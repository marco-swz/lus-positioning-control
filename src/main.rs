use anyhow::{anyhow, Result};
use chrono::Local;
use crossbeam_channel::bounded;
use std::{
    io::Write,
    sync::{Arc, RwLock},
};

mod control;
use control::init;

mod zaber;

mod opcua;
use opcua::run_opcua;

mod utils;
use utils::{Config, ControlStatus, ExecState, SharedState};

mod web;
use web::{run_web_server, WebState};

mod simulation;

fn read_config() -> Result<Config> {
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

fn write_config(config_new: &utils::Config) -> Result<()> {
    return match toml::to_string_pretty(&config_new) {
        Ok(config) => {
            match std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open("config.toml")
            {
                Ok(mut file) => match file.write_all(config.as_bytes()) {
                    Ok(_) => {
                        tracing::debug!("`config.toml` successfully written");
                        Ok(())
                    }
                    Err(e) => {
                        tracing::error!("error writing to `config.toml: {e}");
                        Err(anyhow!("error writing to `config.toml: {e}"))
                    }
                },
                Err(e) => {
                    tracing::error!("error opening `config.toml: {e}");
                    Err(anyhow!("error opening `config.toml: {e}"))
                }
            }
        }
        Err(e) => {
            tracing::error!("error serializing new config: {e}");
            Err(anyhow!("error serializing new config: {e}"))
        }
    };
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

    let config = read_config().unwrap_or_else(|_| {
        let config = Config::default();
        write_config(&config).unwrap();
        config
    });

    let mut state = ExecState {
        shared: shared_state.clone(),
        config: Arc::new(RwLock::new(config.clone())),
        out_channel: Arc::clone(&state_channel),
        rx_stop: rx_stop.clone(),
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

        state.shared.control_state = ControlStatus::Running;
        state.shared.timestamp = Local::now();
        {
            let mut out = state.out_channel.write().unwrap();
            *out = state.shared.clone();
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
