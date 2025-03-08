use crate::{
    utils::{self, ControlStatus, ExecState},
    zaber::{init_zaber, init_zaber_mock, Adc, ManualBackend, TrackingBackend, ZaberConn},
};
use ads1x1x::{channel, Ads1x1x, FullScaleRange, TargetAddr};
use anyhow::{anyhow, Result};
use chrono::Local;
use ftdi_embedded_hal::{libftd2xx, FtHal};
use nb::block;
use std::sync::Arc;

pub trait Backend {
    fn get_target(&mut self) -> Result<(u32, u32, f64, f64)>;
    fn get_pos(&mut self) -> Result<(u32, u32, bool, bool)>;
    fn move_coax(&mut self, target: u32) -> Result<()>;
    fn move_cross(&mut self, target: u32) -> Result<()>;
}

fn init_adc() -> Result<Adc> {
    let Ok(device) = libftd2xx::Ft232h::with_description("Single RS232-HS") else {
        return Err(anyhow!("Failed to open Ft232h"));
    };

    let hal = FtHal::init_freq(device, 400_000).unwrap();
    let Ok(dev) = hal.i2c() else {
        return Err(anyhow!("Failed to create I2C device"));
    };
    let mut adc = Ads1x1x::new_ads1115(dev, TargetAddr::default());
    let Ok(_) = adc.set_full_scale_range(FullScaleRange::Within4_096V) else {
        return Err(anyhow!("Failed set ADC range"));
    };

    return Ok(adc);
}

pub fn init(state: &mut ExecState) -> Result<()> {
    let config = { state.config.read().unwrap().clone() };

    state.shared.error = None;
    if let Ok(mut out) = state.out_channel.try_write() {
        *out = state.shared.clone();
        drop(out);
    }

    tracing::debug!("Init control with backend {:?}", config.control_mode);
    match config.mock_zaber {
        false => init_backend(init_zaber(config)?, state),
        true => init_backend(init_zaber_mock()?, state),
    }
}

fn init_backend<T>(mut port: ZaberConn<T>, state: &mut ExecState) -> Result<()>
where
    T: zproto::backend::Backend,
{
    loop {
        let target_shared = Arc::clone(&state.target_manual);
        let config = {
            let s = state.config.read().unwrap();
            s.clone()
        };

        let result = match config.control_mode {
            utils::ControlMode::Manual => {
                let adc = init_adc()?;
                let backend = ManualBackend::new(
                    &mut port,
                    adc,
                    config.clone(),
                    read_voltage,
                    target_shared,
                )?;
                run(state, backend)
            }

            utils::ControlMode::Tracking => {
                let adc = init_adc()?;
                let backend = TrackingBackend::new(&mut port, config.clone(), adc, read_voltage)?;
                run(state, backend)
            }
        };

        // If only the control mode changes,
        // zaber does not need to re-initalized.
        let config_current = state.config.read().unwrap();
        tracing::debug!(
            "checking control mode: old={:?}, new={:?}",
            config.control_mode,
            config_current.control_mode
        );
        if config.control_mode == config_current.control_mode {
            return result;
        };
    }
}

pub fn run(state: &mut ExecState, mut backend: impl Backend) -> Result<()> {
    let config = state.config.read().unwrap();
    let cycle_time = config.cycle_time_ns;
    let pos_coax_max = config.limit_max_coax;
    let pos_coax_min = config.limit_min_coax;
    let pos_cross_max = config.limit_max_cross;
    let pos_cross_min = config.limit_min_cross;
    drop(config);

    tracing::info!("Starting control loop");
    loop {
        let (target_coax, target_cross, voltage1, voltage2) = backend.get_target()?;
        state.shared.target_coax = target_coax;
        state.shared.target_cross = target_coax;

        let (pos_coax, pos_cross, busy_coax, busy_cross) = backend.get_pos()?;
        state.shared.position_coax = pos_coax;
        state.shared.position_cross = pos_cross;
        state.shared.busy_coax = busy_coax;
        state.shared.busy_cross = busy_cross;
        state.shared.voltage = [voltage1, voltage2];
        state.shared.timestamp = Local::now();

        tracing::debug!("Position coax: target={target_coax} actual={pos_coax}");
        if target_coax > pos_coax_min && target_coax < pos_coax_max && target_coax != pos_coax {
            backend.move_coax(target_coax)?;
        }

        tracing::debug!("Position cross: target={target_cross} actual={pos_cross}");
        if target_cross >= pos_cross_min
            && target_cross <= pos_cross_max
            && target_cross != pos_cross
        {
            backend.move_cross(target_cross)?;
        }

        if let Ok(mut out) = state.out_channel.try_write() {
            *out = state.shared.clone();
            drop(out);
        }

        if let Ok(_) = state.rx_stop.recv_timeout(cycle_time) {
            break;
        }
    }

    tracing::info!("Control loop stopped");
    state.shared.control_state = ControlStatus::Stopped;
    state.shared.timestamp = Local::now();
    let mut out = state.out_channel.write().unwrap();
    *out = state.shared.clone();
    drop(out);
    return Ok(());
}

fn read_voltage(adc: &mut Adc) -> Result<[f64; 2]> {
    let Ok(raw1) = block!(adc.read(channel::DifferentialA0A1)) else {
        return Err(anyhow!("Failed to read from ADC A0A1"));
    };
    let voltage1 = raw1 as f64 * 4.069 / 32767.;

    let Ok(raw2) = block!(adc.read(channel::DifferentialA2A3)) else {
        return Err(anyhow!("Failed to read from ADC A2A3"));
    };
    let voltage2 = raw2 as f64 * 4.069 / 32767.;
    tracing::debug!("voltage read {}, {}", voltage1, voltage2);

    Ok([voltage1, voltage2])
}

#[cfg(test)]
mod tests {
    use std::{sync::RwLock, time::Duration};

    use crossbeam_channel::bounded;
    use utils::{Config, SharedState};

    use super::*;

    fn prepare_state() -> ExecState {
        let (tx_stop, rx_stop) = bounded::<()>(1);
        let (tx_start, rx_start) = bounded::<()>(1);
        let target_manual = Arc::new(RwLock::new((0, 0, 0., 0.)));
        let shared_state = SharedState {
            target_coax: 0,
            target_cross: 0,
            position_cross: 0,
            position_coax: 0,
            busy_cross: false,
            busy_coax: false,
            control_state: ControlStatus::Stopped,
            error: None,
            timestamp: Local::now(),
            voltage: [0.; 2],
        };
        let state_channel = Arc::new(RwLock::new(shared_state.clone()));

        let state = ExecState {
            shared: shared_state,
            config: Arc::new(RwLock::new(Config {
                serial_device: "".to_string(),
                cycle_time_ns: Duration::from_millis(1),
                opcua_config_path: "".into(),
                control_mode: utils::ControlMode::Tracking,
                limit_max_coax: 1000,
                limit_min_coax: 0,
                maxspeed_coax: 10000,
                accel_coax: 10000,
                offset_coax: 0,
                limit_max_cross: 1000,
                limit_min_cross: 0,
                maxspeed_cross: 10000,
                accel_cross: 100000,
                mock_zaber: true,
                formula_coax: "".into(),
                formula_cross: "".into(),
                web_port: 0,
            })),
            rx_stop,
            target_manual,
            out_channel: state_channel,
        };

        return state;
    }

    //#[test]
    fn test_run_stop() {
        let mut port = init_zaber_mock().unwrap();
        // backend.set_reply_callback(|buffer, msg| {
        //     buffer.extend_from_slice(match msg {
        //         b"/1 io get ai 1\n" => b"@01 0 OK BUSY -- 5.5\r\n",
        //         b"/1 lockstep 1 move abs 5\n" => b"@01 0 OK BUSY -- 0\r\n",
        //         b"/get pos\n" => b"@01 0 OK BUSY -- 20\r\n@02 0 OK IDLE -- 10.1\r\n",
        //         e => panic!("unexpected message: {:?}", e),
        //     })
        // });

        //// /io get ai 1
        //backend.push(b"@01 0 OK BUSY -- 5.5\r\n");
        //// /move abs
        //backend.push(b"@01 0 OK BUSY -- 0\r\n");
        //// /get pos
        //backend.push(b"@01 0 OK BUSY -- 20\r\n");
        //backend.push(b"@02 0 OK IDLE -- 10.1\r\n");

        let mut state = prepare_state();
        let adc = init_adc().unwrap();

        let config = { state.config.read().unwrap().clone() };
        let backend = TrackingBackend::new(&mut port, config, adc, read_voltage).unwrap();
        {
            let mut out = state.out_channel.write().unwrap();
            *out = state.shared.clone();
        }

        run(&mut state, backend).unwrap();
    }

    #[test]
    fn testing() {
        let a = 1 + 2;

        assert_eq!(a, 3);
    }
}
