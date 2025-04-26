use std::sync::{Arc, RwLock};

use chrono::Local;
use criterion::{criterion_group, criterion_main, Criterion};
use crossbeam_channel::bounded;
use lus_positioning_control::{
    control::{compute_control, init_adc, read_voltage},
    utils::{Config, ControlStatus, ExecState, SharedState},
    zaber::{init_zaber, init_zaber_mock, ManualBackend, TrackingBackend},
};
use pprof::criterion::{Output, PProfProfiler};

pub fn criterion_benchmark(c: &mut Criterion) {
    let config = Config::default();
    let limits_coax = [config.limit_min_coax, config.limit_max_coax];
    let limits_cross = [config.limit_min_cross, config.limit_max_cross];
    let target_manual = Arc::new(RwLock::new((0, 0, 0., 0.)));
    // let target_shared = Arc::clone(&target_manual);
    // let mut port = init_zaber_mock().unwrap();
    let mut port = init_zaber(config.clone()).unwrap();
    let adc = init_adc().unwrap();
    let mut backend = TrackingBackend::new(&mut port, config.clone(), adc, read_voltage).unwrap();
    let config = Arc::new(RwLock::new(config));
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
        b.iter(|| compute_control(&mut state, &mut backend, limits_coax, limits_cross))
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = criterion_benchmark
}
criterion_main!(benches);
