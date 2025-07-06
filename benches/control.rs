use std::sync::{Arc, RwLock};

use chrono::Local;
use criterion::{criterion_group, criterion_main, Criterion};
use crossbeam_channel::bounded;
use evalexpr::Value;
use lus_positioning_control::{
    control::{compute_control, init_adc, read_voltage},
    utils::{Config, ControlStatus, ExecState, SharedState},
    zaber::{get_pos_zaber, init_zaber, mm_to_steps, move_coax_zaber, move_cross_zaber},
};
use pprof::criterion::{Output, PProfProfiler};

pub fn criterion_benchmark(c: &mut Criterion) {
    let config = Config::default();
    let limits = [
        [config.limit_min_coax, config.limit_max_coax],
        [config.limit_min_cross, config.limit_max_cross]
    ];
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

    let target_manual = Arc::new(RwLock::new([0, 0]));
    let mut port = init_zaber(&config).unwrap();
    let mut adcs = init_adc().unwrap();
    let config = Arc::new(RwLock::new(config));
    let shared_state = SharedState {
        target: [0, 0],
        position: [0, 0],
        is_busy: [false, false],
        control_state: ControlStatus::Stopped,
        error: None,
        timestamp: Local::now(),
        voltage: [0.; 2],
    };
    let state_channel = Arc::new(RwLock::new(shared_state.clone()));
    let (_tx_stop, rx_stop) = bounded::<()>(1);
    let (_tx_start, _rx_start) = bounded::<()>(1);
    let mut state = ExecState {
        shared: shared_state,
        out_channel: state_channel,
        rx_stop,
        target_manual,
        config: Arc::clone(&config),
    };

    c.bench_function("compute_control", |b| {
        b.iter(|| compute_control(
            &mut state, 
            &mut port, 
            &mut adcs, 
            &mut [read_voltage, read_voltage],
            &funcs_voltage_to_target,
            get_pos_zaber,
            &[move_cross_zaber, move_coax_zaber],
            &limits
        ))
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = criterion_benchmark
}
criterion_main!(benches);
