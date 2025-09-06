use anyhow::{anyhow, Result};
use std::{
    fmt::Display,
    io::Write,
    path::PathBuf,
    sync::{Arc, RwLock},
    time::Duration,
};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use crate::zaber::{MAX_POS, MAX_SPEED};

pub type StateChannel = Arc<RwLock<SharedState>>;
pub type StopChannel = tokio::sync::broadcast::Receiver<()>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ControlMode {
    Tracking,
    Manual,
}

fn default_serial_device() -> String {
    "/dev/ttyACM0".into()
}

fn default_opcua_config_path() -> PathBuf {
    "opcua_config.conf".into()
}

fn default_limit_max_coax() -> u32 {
    MAX_POS
}

fn default_limit_min_coax() -> u32 {
    0
}

fn default_maxspeed_coax() -> u32 {
    MAX_SPEED
}

fn default_accel_coax() -> u32 {
    50
}

fn default_control_mode() -> ControlMode {
    ControlMode::Manual
}

fn default_offset_coax() -> i32 {
    0
}

fn default_limit_max_cross() -> u32 {
    MAX_POS
}

fn default_limit_min_cross() -> u32 {
    0
}

fn default_maxspeed_cross() -> u32 {
    MAX_SPEED
}

fn default_accel_cross() -> u32 {
    50
}

fn default_mock_zaber() -> bool {
    false
}

fn default_mock_adc() -> bool {
    false
}

fn default_formula_coax() -> String {
    "64 - (64 - 17) / (2 - 0.12) * (v1 - 0.12)".into()
}

fn default_formula_cross() -> String {
    "0".into()
}

fn default_web_port() -> u32 {
    8085
}

fn default_cycle_time_ms() -> Duration {
    Duration::from_millis(500)
}

pub fn default_config_path() -> String {
    "config.toml".into()
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[serde(default = "default_cycle_time_ms")]
    pub cycle_time_ms: Duration,
    #[serde(default = "default_serial_device")]
    pub serial_device: String,
    #[serde(default = "default_opcua_config_path")]
    pub opcua_config_path: PathBuf,
    #[serde(default = "default_control_mode")]
    pub control_mode: ControlMode,
    #[serde(default = "default_limit_max_coax")]
    pub limit_max_coax: u32,
    #[serde(default = "default_limit_min_coax")]
    pub limit_min_coax: u32,
    #[serde(default = "default_maxspeed_coax")]
    pub maxspeed_coax: u32,
    #[serde(default = "default_accel_coax")]
    pub accel_coax: u32,
    #[serde(default = "default_offset_coax")]
    pub offset_coax: i32,
    #[serde(default = "default_limit_max_cross")]
    pub limit_max_cross: u32,
    #[serde(default = "default_limit_min_cross")]
    pub limit_min_cross: u32,
    #[serde(default = "default_maxspeed_cross")]
    pub maxspeed_cross: u32,
    #[serde(default = "default_accel_cross")]
    pub accel_cross: u32,
    #[serde(default = "default_mock_zaber")]
    pub mock_zaber: bool,
    #[serde(default = "default_mock_adc")]
    pub mock_adc: bool,
    #[serde(default = "default_formula_coax")]
    pub formula_coax: String,
    #[serde(default = "default_formula_cross")]
    pub formula_cross: String,
    #[serde(default = "default_web_port")]
    pub web_port: u32,
    #[serde(default = "default_config_path")]
    pub config_path: String,
}

impl Config {
    pub fn default() -> Self {
        Self {
            cycle_time_ms: default_cycle_time_ms(),
            serial_device: default_serial_device(),
            opcua_config_path: default_opcua_config_path(),
            control_mode: default_control_mode(),
            limit_max_coax: default_limit_max_coax(),
            limit_min_coax: default_limit_min_coax(),
            limit_max_cross: default_limit_max_cross(),
            limit_min_cross: default_limit_min_cross(),
            accel_coax: default_accel_coax(),
            accel_cross: default_accel_cross(),
            maxspeed_cross: default_maxspeed_coax(),
            maxspeed_coax: default_maxspeed_coax(),
            offset_coax: default_offset_coax(),
            mock_zaber: default_mock_zaber(),
            mock_adc: default_mock_adc(),
            formula_coax: default_formula_coax(),
            formula_cross: default_formula_cross(),
            web_port: default_web_port(),
            config_path: default_config_path(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum ControlStatus {
    Stopped,
    Init,
    Running,
    Error,
}

impl Display for ControlStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::Running => "Running",
            Self::Stopped => "Stopped",
            Self::Error => "Error",
            Self::Init => "Initializing",
        };
        write!(f, "{}", text)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SharedState {
    pub target: [u32; 2],
    pub position: [u32; 2],
    pub voltage: [f64; 2],
    pub is_busy: [bool; 2],
    pub control_state: ControlStatus,
    pub error: Option<String>,
    pub timestamp: DateTime<Local>,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            target: [0; 2],
            position: [0; 2],
            is_busy: [false; 2],
            control_state: ControlStatus::Stopped,
            error: None,
            timestamp: Local::now(),
            voltage: [0.; 2],
        }
    }
}

#[derive(Debug)]
pub struct ExecState {
    pub shared: SharedState,
    pub out_channel: StateChannel,
    pub tx_stop: tokio::sync::broadcast::Sender<()>,
    pub target_manual: Arc<RwLock<[u32; 2]>>,
    pub config: Arc<RwLock<Config>>,
}

impl ExecState {
    pub fn set_error(&mut self, text: String) {
        self.shared.control_state = ControlStatus::Error;
        self.shared.error = Some(text);
        self.shared.timestamp = Local::now();
        let mut out = self.out_channel.write().unwrap();
        *out = self.shared.clone();
    }

    pub fn set_status(&mut self, status: ControlStatus) {
        self.shared.control_state = status;
        self.shared.timestamp = Local::now();
        let mut out = self.out_channel.write().unwrap();
        *out = self.shared.clone();
    }
}

impl Default for ExecState {
    fn default() -> Self {
        let (tx_stop, _rx_stop) = tokio::sync::broadcast::channel(1);
        let shared = SharedState::default();
        let out_channel = Arc::new(RwLock::new(shared.clone()));
        Self {
            shared,
            out_channel,
            tx_stop,
            target_manual: Arc::new(RwLock::new([0; 2])),
            config: Arc::new(RwLock::new(Config::default())),
        }
    }
}

pub fn read_config() -> Config {
    let config_file = std::env::args().skip(1).next().unwrap_or(default_config_path());

    let mut config = match std::fs::read_to_string(&config_file) {
        Ok(config) => {
            tracing::debug!("`{}` successfully read", config_file);

            match toml::from_str::<Config>(&config) {
                Ok(config) => {
                    tracing::debug!("`{}` successfully parsed", config_file);
                    config
                }
                Err(e) => {
                    tracing::error!("error parsing `{}`: {}", config_file, e);
                    Config::default()
                }
            }
        }
        Err(e) => {
            tracing::error!("error loading `{}`: {}", config_file, e);
            Config::default()
        }
    };
    config.config_path = config_file;
    return config;
}

pub fn write_config(config_new: &Config) -> Result<()> {
    return match toml::to_string_pretty(&config_new) {
        Ok(config) => {
            match std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(config_new.config_path.clone())
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
