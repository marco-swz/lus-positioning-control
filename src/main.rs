use std::sync::{Arc, RwLock};
use std::time::Duration;
use chrono::Local;
use crossbeam_channel::bounded;

mod control;
use control::{init, Backend, Config, ControlStatus, ExecState, SharedState};

mod zaber;
mod ramp;

mod opcua;
use opcua::run_opcua;

mod web;
use web::{run_web_server, WebState};

fn main() {
    tracing_subscriber::fmt::init();

    let (tx_stop, rx_stop) = bounded::<()>(1);
    let (tx_start, rx_start) = bounded::<()>(1);

    let shared_state = SharedState {
        voltage_gleeble: 0.,
        position_cross: 0.,
        position_parallel: 0.,
        busy_cross: false,
        busy_parallel: false,
        control_state: ControlStatus::Stopped,
        error: None,
        timestamp: Local::now(),
    };
    let state_channel = Arc::new(RwLock::new(shared_state.clone()));

    let mut state = ExecState {
        shared: shared_state.clone(),
        config: Arc::new(RwLock::new(Config {
            cycle_time: Duration::from_millis(1000),
            restart_timeout: Duration::from_secs(10),
            voltage_min: 5.,
            voltage_max: 100.,
            serial_device: "/dev/ttyACM0".to_string(),
            opcua_config_path: "opcua_config.conf".into(),
            backend: Backend::Ramp,
        })),
        out_channel: Arc::clone(&state_channel),
        rx_stop: rx_stop.clone(),
    };


    let queue_clone = Arc::clone(&state_channel);
    let opcua_state = {
        let config_path = state.config.read().unwrap().opcua_config_path.clone();
        run_opcua(queue_clone, config_path)
    };

    let web_state = WebState{
        zaber_state: state_channel,
        tx_stop_control: tx_stop.clone(),
        tx_start_control: tx_start.clone(),
        config: state.config.clone(),
        opcua_state,
    };
    std::thread::spawn(|| run_web_server(web_state));

    let mut out = state.out_channel.write().unwrap();
    *out = shared_state.clone();
    drop(out);

    let mut stopped = true;
    loop {
        if let Ok(_) = rx_stop.try_recv() {
            stopped = true;
        }

        if stopped {
            state.shared.control_state = ControlStatus::Stopped;
            {
                let mut out = state.out_channel.write().unwrap();
                *out = state.shared.clone();
            }
            tracing::debug!("control stopped - wait for start");
            let _ = rx_start.recv();
        }

        tracing::debug!("trying to init control");
        match init(&mut state) {
            Ok(_) => stopped = true,
            Err(e) => {
                tracing::error!("control error: {}", &e);
                state.shared.control_state = ControlStatus::Error;
                state.shared.error = Some(e.to_string());
                state.shared.timestamp = Local::now();

                {
                    let mut out = state.out_channel.write().unwrap();
                    *out = state.shared.clone();
                }

                let restart_timeout = {
                    state.config.read().unwrap().restart_timeout
                };
                std::thread::sleep(restart_timeout);
                stopped = false;
            }
        }
    }
}
