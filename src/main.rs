use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::Duration;

mod methods;

mod control;
use control::{init, Config, ControlState, ExecState, SharedState};

mod zaber;

mod opcua;
use opcua::run_opcua;

mod web;
use web::run_web_server;

fn main() {
    let stop_channel = Arc::new((Mutex::new(false), Condvar::new()));
    let shared_state = SharedState {
        voltage_gleeble: 0.,
        position_cross: 0.,
        position_parallel: 0.,
        busy_cross: false,
        busy_parallel: false,
        control_state: ControlState::Disconnected,
        error: None,
    };
    let state_channel = Arc::new(RwLock::new(shared_state.clone()));

    let queue_clone = Arc::clone(&state_channel);
    let stop_clone = Arc::clone(&stop_channel);
    std::thread::spawn(|| run_opcua(queue_clone, stop_clone));
    let queue_clone = Arc::clone(&state_channel);
    let stop_clone = Arc::clone(&stop_channel);
    std::thread::spawn(|| run_web_server(queue_clone, stop_clone));

    let mut state = ExecState {
        shared: shared_state.clone(),
        config: Config {
            cycle_time_ms: 1000,
            voltage_min: 5.,
            voltage_max: 100.,
            serial_device: "/dev/ttyACM0".to_string(),
        },
        out_channel: state_channel,
        stop_channel: Arc::clone(&stop_channel),
    };

    let mut out = state.out_channel.write().unwrap();
    *out = shared_state.clone();
    drop(out);

    loop {
        let (lock, cvar) = &*stop_channel;
        let mut stop = lock.lock().unwrap();
        if *stop {
            let result = cvar.wait_timeout(stop, Duration::from_secs(5)).unwrap();
            stop = result.0;
        }

        drop(stop);

        continue;

        match init(&mut state) {
            Ok(_) => {}
            Err(e) => {
                println!("{}", e);
                state.shared.control_state = ControlState::Disconnected;
                state.shared.error = Some(e.to_string());
                let mut out = state.out_channel.write().unwrap();
                *out = state.shared.clone();
                drop(out);
            }
        }
    }
}
