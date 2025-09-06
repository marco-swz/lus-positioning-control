use crate::{
    adc::AdcBackend,
    utils::{self, ExecState},
    zaber::{mm_to_steps, AxisBackend},
};
use anyhow::Result;
use evalexpr::Value;
use std::sync::Arc;

pub fn get_voltage_conversion(
    state: &mut ExecState,
) -> Result<[Box<dyn Fn(&[f64; 2]) -> Result<u32>>; 2]> {
    let config = { state.config.read().unwrap().clone() };
    match config.control_mode {
        utils::ControlMode::Manual => {
            tracing::debug!("starting in control mode Manual");
            return Ok([
                Box::new(get_voltage_conversion_manual(state, 0)?),
                Box::new(get_voltage_conversion_manual(state, 1)?),
            ]);
        }

        utils::ControlMode::Tracking => {
            return Ok([
                Box::new(get_voltage_conversion_formula(&config.formula_coax)?),
                Box::new(get_voltage_conversion_formula(&config.formula_cross)?),
            ])
        }
    }
}

fn get_voltage_conversion_manual(
    state: &mut ExecState,
    axis_index: usize,
) -> Result<impl Fn(&[f64; 2]) -> Result<u32>> {
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

        if target >= limits[i][0] && target <= limits[i][1] && target != positions[i] {
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
    use crate::{adc::MockAdcModule, zaber::MockZaberPort};

    use super::*;

    #[test]
    fn test_voltage_conversion() {
        let mut state = ExecState::default();
        {
            let mut config = state.config.write().unwrap();
            config.formula_coax = "v1 * v2".into();
            config.formula_cross = "v1 + v2".into();
            config.control_mode = utils::ControlMode::Tracking;
        }
        let funcs = get_voltage_conversion(&mut state).unwrap();
        assert_eq!((funcs[0])(&[5., 2.]).unwrap(), mm_to_steps(10.));
        assert_eq!((funcs[1])(&[5., 2.]).unwrap(), mm_to_steps(7.));
    }

    #[test]
    fn test_run_stop() {
        let mut state = ExecState::default();
        {
            let mut config = state.config.write().unwrap();
            config.formula_coax = "v1 * v2".into();
            config.formula_cross = "v1 + v2".into();
            config.control_mode = utils::ControlMode::Tracking;
        }

        let adc_module = MockAdcModule::new(Box::new(|| Ok([5., 2.]))).unwrap();
        let axis_port = MockZaberPort::new(
            &state.config.read().unwrap().clone(),
            state.tx_stop.subscribe(),
        );

        let funcs = get_voltage_conversion(&mut state).unwrap();
        {
            let mut out = state.out_channel.write().unwrap();
            *out = state.shared.clone();
        }

        state.tx_stop.send(()).unwrap();

        run_control_loop(
            &mut state,
            Box::new(axis_port.unwrap().unwrap()),
            Box::new(adc_module),
            funcs,
        ).unwrap();

        let out = state.out_channel.read().unwrap();
        assert_eq!(out.error, None);
    }
}
