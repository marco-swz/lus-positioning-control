use crate::{
    utils::{self, Config, ControlStatus, ExecState},
    zaber::{
        get_pos_zaber, init_zaber, init_zaber_mock, mm_to_steps, move_coax_zaber, move_cross_zaber,
        Adc, ZaberConn,
    },
};
use ads1x1x::{Ads1x1x, FullScaleRange, TargetAddr};
use anyhow::{anyhow, Result};
use chrono::Local;
use evalexpr::Value;
use ftdi_embedded_hal::{libftd2xx, FtHal};
use std::sync::Arc;

pub trait Backend {
    fn get_target(&mut self) -> Result<(u32, u32, f64, f64)>;
    fn get_pos(&mut self) -> Result<(u32, u32, bool, bool)>;
    fn move_coax(&mut self, target: u32) -> Result<()>;
    fn move_cross(&mut self, target: u32) -> Result<()>;
}

pub fn init_adc() -> Result<[Adc; 2]> {
    let adcs: [Result<Adc>; 2] = ["", ""].map(|serial_number| {
        let Ok(device) = libftd2xx::Ft232h::with_serial_number(serial_number) else {
            return Err(anyhow!("Failed to open Ft232h"));
        };

        let hal = FtHal::init_freq(device, 400_000).unwrap();
        let Ok(dev) = hal.i2c() else {
            return Err(anyhow!("Failed to create I2C device"));
        };
        let adc = Ads1x1x::new_ads1115(dev, TargetAddr::default());
        let Ok(mut adc) = adc.into_continuous() else {
            return Err(anyhow!("Failed set ADC continuous mode"));
        };
        let Ok(_) = adc.set_full_scale_range(FullScaleRange::Within4_096V) else {
            return Err(anyhow!("Failed set ADC range"));
        };

        return Ok(adc);
    });

    for adc in &adcs {
        match adc {
            Err(e) => return anyhow::bail!(e.to_string()),
            Ok(_) => (),
        };
    }
    return Ok(adcs.map(|adc| adc.unwrap()));
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
        false => match config.mock_adc {
            false => init_backend(init_zaber(&config)?, state, read_voltage_adc(&config)?),
            true => init_backend(init_zaber(&config)?, state, read_voltage_mock()?),
        },
        true => match config.mock_adc {
            false => init_backend(init_zaber_mock()?, state, read_voltage_adc(&config)?),
            true => init_backend(init_zaber_mock()?, state, read_voltage_mock()?),
        },
    }
}

fn read_voltage_adc(config: &Config) -> Result<[impl FnMut() -> Result<f64>; 2]> {
    let adcs = init_adc().unwrap();
    Ok(adcs.map(|mut adc| move || read_voltage(&mut adc)))
}

fn read_voltage_mock() -> Result<[impl FnMut() -> Result<f64>; 2]> {
    let a = 0.;
    Ok([0, 0].map(|_| move || Ok(a)))
}

fn init_backend<T>(
    mut port: ZaberConn<T>,
    state: &mut ExecState,
    mut funcs_read_voltage: [impl FnMut() -> Result<f64>; 2],
) -> Result<()>
where
    T: zproto::backend::Backend,
{
    loop {
        let config = {
            let s = state.config.read().unwrap();
            s.clone()
        };

        let result = match config.control_mode {
            utils::ControlMode::Manual => {
                let funcs_voltage_to_target = [0, 1].map(|i| {
                    let targets_shared = Arc::clone(&state.target_manual);
                    move |_voltages: &[f64; 2]| {
                        let targets = targets_shared.read().unwrap();

                        return Ok(targets[i]);
                    }
                });
                run(
                    state,
                    &mut port,
                    &mut funcs_read_voltage,
                    funcs_voltage_to_target,
                    get_pos_zaber,
                    [move_cross_zaber, move_coax_zaber],
                )
            }

            utils::ControlMode::Tracking => {
                let funcs_voltage_to_target = [
                    evalexpr::build_operator_tree(&config.formula_cross)?,
                    evalexpr::build_operator_tree(&config.formula_coax)?,
                ]
                .map(|f: evalexpr::Node<evalexpr::DefaultNumericTypes>| {
                    move |voltages: &[f64; 2]| {
                        let context = evalexpr::context_map! {
                            "v1" => Value::Float(voltages[0]),
                            "v2" => Value::Float(voltages[1]),
                        }?;

                        let target = f.eval_number_with_context(&context)?;
                        let target = mm_to_steps(target);

                        return Ok(target);
                    }
                });

                run(
                    state,
                    &mut port,
                    &mut funcs_read_voltage,
                    funcs_voltage_to_target,
                    get_pos_zaber,
                    [move_cross_zaber, move_coax_zaber],
                )
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

pub fn run<T>(
    mut state: &mut ExecState,
    mut backend: &mut T,
    mut funcs_read_voltage: &mut [impl FnMut() -> Result<f64>; 2],
    funcs_voltage_to_target: [impl Fn(&[f64; 2]) -> Result<u32>; 2],
    func_get_pos: fn(&mut T) -> Result<([bool; 2], [u32; 2])>,
    funcs_move: [fn(&mut T, u32) -> Result<()>; 2],
) -> Result<()> {
    let config = state.config.read().unwrap();
    let cycle_time = config.cycle_time_ms;
    let limits = [
        [config.limit_min_coax, config.limit_max_coax],
        [config.limit_min_cross, config.limit_max_cross],
    ];
    drop(config);

    tracing::info!("Starting control loop");
    loop {
        compute_control::<T>(
            &mut state,
            &mut backend,
            &mut funcs_read_voltage,
            &funcs_voltage_to_target,
            func_get_pos,
            &funcs_move,
            &limits,
        )?;

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

#[inline]
pub fn compute_control<T>(
    state: &mut ExecState,
    backend: &mut T,
    funcs_read_voltage: &mut [impl FnMut() -> Result<f64>; 2],
    funcs_voltage_to_target: &[impl Fn(&[f64; 2]) -> Result<u32>; 2],
    func_get_pos: fn(&mut T) -> Result<([bool; 2], [u32; 2])>,
    funcs_move: &[fn(&mut T, u32) -> Result<()>; 2],
    limits: &[[u32; 2]; 2],
) -> Result<()> {
    let voltage_readings: Vec<Result<f64>> = funcs_read_voltage
        .iter_mut()
        .map(|func_read_voltage| func_read_voltage())
        .collect();

    let (is_busy, positions) = func_get_pos(backend)?; // TODO(marco): Run in parallel

    // Just to convert into [f64; 2]
    let mut voltages = [0.; 2];
    for (i, v) in voltage_readings.into_iter().enumerate() {
        voltages[i] = v?;
    }

    for i in 0..2 {
        let target = funcs_voltage_to_target[i](&voltages)?;
        state.shared.position[i] = positions[i];
        state.shared.is_busy[i] = is_busy[i];
        state.shared.voltage[i] = voltages[i];
        state.shared.target[i] = target;

        tracing::debug!("Position {}: target={} actual={}", i, target, positions[i]);

        if target > limits[i][0] && target < limits[i][1] && target != positions[i] {
            (funcs_move[i])(backend, target)?;
        }
    }

    if let Ok(mut out) = state.out_channel.try_write() {
        *out = state.shared.clone();
        drop(out);
    }

    return Ok(());
}

pub fn read_voltage(adc: &mut Adc) -> Result<f64> {
    let Ok(raw) = adc.read() else {
        return Err(anyhow!("Failed to read from ADC"));
    };
    let voltage = raw as f64 * 4.069 / 32767.;

    tracing::debug!("voltage read {}", voltage);

    Ok(voltage)
}

#[cfg(test)]
mod tests {
    use std::{sync::RwLock, time::Duration};

    use crossbeam_channel::bounded;
    use utils::{Config, SharedState};

    use super::*;

    fn prepare_state() -> ExecState {
        let (_tx_stop, rx_stop) = bounded::<()>(1);
        let (_tx_start, _rx_start) = bounded::<()>(1);
        let target_manual = Arc::new(RwLock::new([0; 2]));
        let shared_state = SharedState {
            target: [0; 2],
            position: [0; 2],
            is_busy: [false; 2],
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
                cycle_time_ms: Duration::from_millis(1),
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
                mock_adc: true,
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

    #[test]
    fn test_run_stop() {
        let mut port = init_zaber_mock().unwrap();

        let mut state = prepare_state();

        let config = { state.config.read().unwrap().clone() };
        {
            let mut out = state.out_channel.write().unwrap();
            *out = state.shared.clone();
        }

        let adcs = init_adc().unwrap();
        let mut funcs_read_voltage = adcs.map(|mut adc| move || read_voltage(&mut adc));
        let funcs_voltage_to_target = [
            evalexpr::build_operator_tree(&config.formula_cross).unwrap(),
            evalexpr::build_operator_tree(&config.formula_coax).unwrap(),
        ]
        .map(|f: evalexpr::Node<evalexpr::DefaultNumericTypes>| {
            move |voltages: &[f64; 2]| {
                let context = evalexpr::context_map! {
                    "v1" => Value::Float(voltages[0]),
                    "v2" => Value::Float(voltages[1]),
                }?;

                let target = f.eval_number_with_context(&context)?;
                let target = mm_to_steps(target);

                return Ok(target);
            }
        });
        run(
            &mut state,
            &mut port,
            &mut funcs_read_voltage,
            funcs_voltage_to_target,
            get_pos_zaber,
            [move_cross_zaber, move_coax_zaber],
        )
        .unwrap();
    }

    #[test]
    fn testing() {
        let a = 1 + 2;

        assert_eq!(a, 3);
    }
}
