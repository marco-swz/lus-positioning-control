use crate::{
    adc::AdcBackend,
    utils::{self, Config, ExecState},
    zaber::{mm_to_steps, AxisBackend},
};
use anyhow::Result;
use evalexpr::Value;
use std::sync::Arc;

pub fn get_voltage_conversion(
    state: &mut ExecState,
    config: &Config,
) -> Result<[Box<dyn Fn(&[f64; 2]) -> Result<u32>>; 2]> {
    match config.control_mode {
        utils::ControlMode::Manual => {
            tracing::debug!("starting in control mode Manual");
            return Ok([
                Box::new(get_voltage_conversion_manual(state, 0)?),
                Box::new(get_voltage_conversion_manual(state, 1)?),
            ])
        }

        utils::ControlMode::Tracking => {
            return Ok([
                Box::new(get_voltage_conversion_formula(&config.formula_coax)?),
                Box::new(get_voltage_conversion_formula(&config.formula_cross)?),
            ])
        }
    }
}

fn get_voltage_conversion_manual(state: &mut ExecState, axis_index: usize) -> Result<impl Fn(&[f64; 2]) -> Result<u32>> {
    let targets_shared = Arc::clone(&state.target_manual);
    let func = move |_voltages: &[f64; 2]| {
        let targets = targets_shared.read().unwrap();

        return Ok(targets[axis_index]);
    };
    return Ok(func);
}

fn get_voltage_conversion_formula(formula: &str) -> Result<impl Fn(&[f64; 2]) -> Result<u32>> {
    tracing::debug!("starting in control mode Tracking");
    let formula: evalexpr::Node<evalexpr::DefaultNumericTypes> =
        evalexpr::build_operator_tree(formula)?;
    let func = move |voltages: &[f64; 2]| {
        let context = evalexpr::context_map! {
            "v1" => Value::Float(voltages[0]),
            "v2" => Value::Float(voltages[1]),
        }?;

        let target = formula.eval_number_with_context(&context)?;
        let target = mm_to_steps(target);

        return Ok(target);
    };
    return Ok(func);
}

pub fn run_control_loop(
    mut state: &mut ExecState,
    mut axis_backend: Box<dyn AxisBackend>,
    mut adc_backend: Box<dyn AdcBackend>,
    funcs_voltage_to_target: [impl Fn(&[f64; 2]) -> Result<u32>; 2],
) -> Result<()> {
    let config = state.config.read().unwrap();
    let cycle_time = config.cycle_time_ms;
    let limits = [
        [config.limit_min_coax, config.limit_max_coax],
        [config.limit_min_cross, config.limit_max_cross],
    ];
    drop(config);

    let mut rx_stop = state.tx_stop.subscribe();

    tracing::info!("Starting control loop");
    loop {
        compute_control(
            &mut state,
            &mut axis_backend,
            &mut adc_backend,
            &funcs_voltage_to_target,
            &limits,
        )?;

        std::thread::sleep(cycle_time);
        if let Ok(_) = rx_stop.try_recv() {
            break;
        }
    }

    tracing::info!("Control loop stopped");
    return Ok(());
}

#[inline]
pub fn compute_control(
    state: &mut ExecState,
    axis_backend: &mut Box<dyn AxisBackend>,
    adc_backend: &mut Box<dyn AdcBackend>,
    funcs_voltage_to_target: &[impl Fn(&[f64; 2]) -> Result<u32>; 2],
    limits: &[[u32; 2]; 2],
) -> Result<()> {
    let voltages = adc_backend.read_voltage()?;

    let (is_busy, positions) = axis_backend.get_pos()?;

    for i in 0..2 {
        let target = funcs_voltage_to_target[i](&voltages)?;
        state.shared.position[i] = positions[i];
        state.shared.is_busy[i] = is_busy[i];
        state.shared.voltage[i] = voltages[i];
        state.shared.target[i] = target;

        tracing::debug!("Position {}: target={} actual={}", i, target, positions[i]);

        if target > limits[i][0] && target < limits[i][1] && target != positions[i] {
            axis_backend.move_axis(i + 1, target)?
        }
    }

    if let Ok(mut out) = state.out_channel.try_write() {
        *out = state.shared.clone();
        drop(out);
    }

    return Ok(());
}

#[cfg(test)]
mod tests {
    use std::{sync::RwLock, time::Duration};

    use chrono::Local;
    use crossbeam_channel::bounded;
    use utils::{Config, SharedState};

    use crate::{adc::MockAdcModule, utils::ControlStatus};

    use super::*;

    fn prepare_state() -> ExecState {
        let (tx_stop, _rx_stop) = tokio::sync::broadcast::channel(1);
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
                formula_coax: "v1 + v2".into(),
                formula_cross: "v1 + v2".into(),
                web_port: 0,
            })),
            tx_stop,
            target_manual,
            out_channel: state_channel,
        };

        return state;
    }

    // #[test]
    //fn test_run_stop() {
    //    let mut state = prepare_state();

    //    let config = { state.config.read().unwrap().clone() };
    //    {
    //        let mut out = state.out_channel.write().unwrap();
    //        *out = state.shared.clone();
    //    }
    //    let adc_module = MockAdcModule::new(&config).unwrap();

    //    let funcs_voltage_to_target = [
    //        evalexpr::build_operator_tree(&config.formula_cross).unwrap(),
    //        evalexpr::build_operator_tree(&config.formula_coax).unwrap(),
    //    ]
    //    .map(|f: evalexpr::Node<evalexpr::DefaultNumericTypes>| {
    //        move |voltages: &[f64; 2]| {
    //            let context = evalexpr::context_map! {
    //                "v1" => Value::Float(voltages[0]),
    //                "v2" => Value::Float(voltages[1]),
    //            }?;

    //            let target = f.eval_number_with_context(&context)?;
    //            let target = mm_to_steps(target);

    //            return Ok(target);
    //        }
    //    });
    //    run(
    //        &mut state,
    //        &mut port,
    //        &mut [0., 0.],
    //        &mut [read_voltage_mock, read_voltage_mock],
    //        funcs_voltage_to_target,
    //        get_pos_zaber,
    //        [move_coax_zaber, move_cross_zaber],
    //    )
    //    .unwrap();
    //}
}
