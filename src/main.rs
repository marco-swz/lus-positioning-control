use std::sync::{Arc, RwLock};
use std::time::Duration;
use crossbeam_channel::bounded;

mod methods;

mod control;
use control::{init, Config, ControlState, ExecState, SharedState};

mod zaber;

mod opcua;
use opcua::run_opcua;

mod web;
use web::run_web_server;

fn main() {
    let (tx_stop, rx_stop) = bounded::<()>(1);
    let (tx_start, rx_start) = bounded::<()>(1);

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
    std::thread::spawn(|| run_opcua(queue_clone));

    let queue_clone = Arc::clone(&state_channel);
    let tx_stop_clone = tx_stop.clone();
    let tx_start_clone = tx_start.clone();
    std::thread::spawn(|| run_web_server(queue_clone, tx_start_clone, tx_stop_clone));

    let mut state = ExecState {
        shared: shared_state.clone(),
        config: Config {
            cycle_time: Duration::from_millis(1000),
            restart_timeout: Duration::from_secs(10),
            voltage_min: 5.,
            voltage_max: 100.,
            serial_device: "/dev/ttyACM0".to_string(),
        },
        out_channel: state_channel,
        rx_stop: rx_stop.clone(),
    };

    let mut out = state.out_channel.write().unwrap();
    *out = shared_state.clone();
    drop(out);

    let mut stopped = true;
    loop {
        if stopped {
            if let Ok(_) = rx_start.recv() {
                // TODO(marco)
                continue;
            }
        }

        match init(&mut state) {
            Ok(_) => stopped = true,
            Err(e) => {
                println!("{}", e);
                state.shared.control_state = ControlState::Disconnected;
                state.shared.error = Some(e.to_string());
                let mut out = state.out_channel.write().unwrap();
                *out = state.shared.clone();
                drop(out);
                std::thread::sleep(state.config.restart_timeout);
                stopped = false;
            }
        }
    }
}
