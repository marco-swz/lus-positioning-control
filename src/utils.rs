use std::{fmt::Display, path::PathBuf, sync::{Arc, RwLock}, time::Duration};

use crossbeam_channel::Receiver;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

pub type StateChannel = Arc<RwLock<SharedState>>;
pub type StopChannel = Receiver<()>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Backend {
    Zaber,
    Ramp,
    Tracking,
    Manual,
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    pub cycle_time: Duration,
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    pub restart_timeout: Duration,
    pub voltage_min: f64,
    pub voltage_max: f64,
    pub serial_device: String,
    pub opcua_config_path: PathBuf,
    pub backend: Backend,
    pub limit_max_coax: f64,
    pub limit_min_coax: f64,
    pub maxspeed_coax: f64,
    pub offset_coax: f64,
    pub limit_max_cross: f64,
    pub limit_min_cross: f64,
    pub maxspeed_cross: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum ControlStatus {
    Stopped,
    Running,
    Error,
}

impl Display for ControlStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::Running => "Running",
            Self::Stopped => "Stopped",
            Self::Error => "Error",
        };
        write!(f, "{}", text)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SharedState {
    pub target_coax: f64,
    pub target_cross: f64,
    pub position_cross: f64,
    pub position_coax: f64,
    pub busy_cross: bool,
    pub busy_coax: bool,
    pub control_state: ControlStatus,
    pub error: Option<String>,
    pub timestamp: DateTime<Local>,
}

#[derive(Debug)]
pub struct ExecState {
    pub shared: SharedState,
    pub out_channel: StateChannel,
    pub rx_stop: StopChannel,
    pub target_manual: Arc<RwLock<(f64, f64)>>,
    pub config: Arc<RwLock<Config>>,
}

