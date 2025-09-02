use crossbeam_channel::bounded;
use std::sync::{Arc, RwLock};
use anyhow::Result;

use lus_positioning_control::{
    adc::{get_adc_module, AdcBackend},
    control::{get_voltage_conversion, run_control_loop},
    opcua::run_opcua,
    utils::{read_config, write_config, Config, ControlStatus, ExecState, SharedState},
    web::{run_web_server, WebState},
    zaber::{get_axis_port, AxisBackend},
};

fn init_backend(
    config: Config,
    rx_stop: tokio::sync::broadcast::Receiver<()>,
) -> Result<Option<(Box<dyn AxisBackend + Send>, Box<dyn AdcBackend + Send>)>> {
    let Some(axis_backend) = get_axis_port(&config, rx_stop)? else {
        return Ok(None);
    };
    let adc_backend = get_adc_module(&config)?;

    return Ok(Some((axis_backend, adc_backend)));
}

fn main() {
    tracing_subscriber::fmt::init();

    let (tx_stop, mut rx_stop) = tokio::sync::broadcast::channel(1);
    let (tx_start, rx_start) = bounded::<()>(1);

    let target_manual = Arc::new(RwLock::new([0; 2]));

    let shared_state = SharedState::default();
    let state_channel = Arc::new(RwLock::new(shared_state.clone()));

    let config = read_config().unwrap_or_else(|_| {
        let config = Config::default();
        write_config(&config).unwrap();
        config
    });


    let mut state = ExecState {
        shared: shared_state.clone(),
        config: Arc::new(RwLock::new(config.clone())),
        out_channel: Arc::clone(&state_channel),
        tx_stop: tx_stop.clone(),
        target_manual: Arc::clone(&target_manual),
    };

    let queue_clone = Arc::clone(&state_channel);
    let config_path = state.config.read().unwrap().opcua_config_path.clone();
    run_opcua(queue_clone, config_path);

    let web_state = WebState {
        zaber_state: state_channel,
        tx_stop_control: tx_stop.clone(),
        tx_start_control: tx_start.clone(),
        config: state.config.clone(),
        target_manual,
    };
    std::thread::spawn(|| run_web_server(web_state));

    let mut out = state.out_channel.write().unwrap();
    *out = shared_state.clone();
    drop(out);

    state.shared.control_state = ControlStatus::Stopped;
    loop {
        tracing::debug!("control waiting for start");
        let _ = rx_start.recv();
        tracing::debug!("start signal received");

        // There might be more signals in channel,
        // they need to be cleared.
        while !rx_stop.is_empty() {
            let _ = rx_stop.blocking_recv();
        }
        while !rx_start.is_empty() {
            let _ = rx_start.try_recv();
        }

        state.set_status(ControlStatus::Init);

        let funcs_voltage_to_target = get_voltage_conversion(&mut state).unwrap();
        let rx_stop = state.tx_stop.subscribe();
        let config = { state.config.read().unwrap().clone() };

        tracing::debug!("trying to init backend");
        let backend = match init_backend(config.clone(), rx_stop) {
            Err(e) => {
                state.set_error(e.to_string());
                continue;
            }
            Ok(b) => b,
        };

        let Some(backend) = backend else {
            state.set_status(ControlStatus::Stopped);
            continue;
        };

        let (axis_backend, adc_backend) = backend;

        state.set_status(ControlStatus::Running);
        match run_control_loop(&mut state, axis_backend, adc_backend, funcs_voltage_to_target) {
            Ok(_) => {
                state.set_status(ControlStatus::Stopped);
            }
            Err(e) => {
                tracing::error!("control error: {}", &e);
                state.set_error(e.to_string());
            }
        }
    }
}
