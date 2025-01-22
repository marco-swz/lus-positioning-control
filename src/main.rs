use anyhow::Result;
use chrono::Local;
use crossbeam_channel::bounded;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zaber::{steps_per_sec_to_mm_per_sec, MAX_POS, MAX_SPEED};

mod control;
use control::init;

mod zaber;

mod opcua;
use opcua::run_opcua;

mod utils;
use utils::{Config, ControlMode, ControlStatus, ExecState, SharedState};

mod web;
use web::{run_web_server, WebState};

mod simulation;

fn read_config() -> Result<Config> {
    // TODO(marco): Panic on parse error, default if none found
    match std::fs::read_to_string("config.toml") {
        Ok(config) => {
            tracing::debug!("`config.toml` successfully read");

            match toml::from_str(&config) {
                Ok(config) => {
                    tracing::debug!("`config.toml` successfully parsed");
                    Ok(config)
                }
                Err(e) => {
                    tracing::error!("error parsing `config.toml: {}", e);
                    Err(e.into())
                }
            }
        }
        Err(e) => {
            tracing::error!("error loading `config.toml: {}", e);
            Err(e.into())
        }
    }
}

fn main() {
    tracing_subscriber::fmt::init();

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

    let config_default = Config {
        cycle_time_ns: Duration::from_millis(500),
        serial_device: "/dev/ttyACM0".to_string(),
        opcua_config_path: "opcua_config.conf".into(),
        control_mode: ControlMode::Manual,
        limit_max_coax: MAX_POS,
        limit_min_coax: 0,
        limit_max_cross: MAX_POS,
        limit_min_cross: 0,
        maxspeed_cross: steps_per_sec_to_mm_per_sec(MAX_SPEED),
        maxspeed_coax: steps_per_sec_to_mm_per_sec(MAX_SPEED),
        offset_coax: 0,
        mock_zaber: true,
        formula_coax: "64 - (64 - 17) / (2 - 0.12) * (v1 - 0.12)".to_string(),
        formula_cross: "0".to_string(),
    };

    let config = read_config().unwrap_or(config_default);

    let mut state = ExecState {
        shared: shared_state.clone(),
        config: Arc::new(RwLock::new(config.clone())),
        out_channel: Arc::clone(&state_channel),
        rx_stop: rx_stop.clone(),
        target_manual: Arc::clone(&target_manual),
    };

    let queue_clone = Arc::clone(&state_channel);
    let opcua_state = {
        let config_path = state.config.read().unwrap().opcua_config_path.clone();
        run_opcua(queue_clone, config_path)
    };

    let web_state = WebState {
        zaber_state: state_channel,
        tx_stop_control: tx_stop.clone(),
        tx_start_control: tx_start.clone(),
        config: state.config.clone(),
        opcua_state,
        target_manual,
    };
    std::thread::spawn(|| run_web_server(web_state));

    let mut out = state.out_channel.write().unwrap();
    *out = shared_state.clone();
    drop(out);

    loop {
        state.shared.control_state = ControlStatus::Stopped;
        {
            let mut out = state.out_channel.write().unwrap();
            *out = state.shared.clone();
        }
        tracing::debug!("control waiting for start");
        let _ = rx_start.recv();
        tracing::debug!("start signal received");

        // There might be more signals in channel,
        // they need to be cleared.
        while !state.rx_stop.is_empty() {
            let _ = state.rx_stop.try_recv();
        }
        while !rx_start.is_empty() {
            let _ = rx_start.try_recv();
        }

        tracing::debug!("trying to init control");
        match init(&mut state) {
            Ok(_) => (),
            Err(e) => {
                tracing::error!("control error: {}", &e);
                state.shared.control_state = ControlStatus::Error;
                state.shared.error = Some(e.to_string());
                state.shared.timestamp = Local::now();

                {
                    let mut out = state.out_channel.write().unwrap();
                    *out = state.shared.clone();
                }
            }
        }
    }
}
