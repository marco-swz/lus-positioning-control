use std::{
    fmt::Display,
    path::PathBuf,
    sync::{Arc, RwLock},
    time::Duration,
};

use chrono::{DateTime, Local};
use crossbeam_channel::Receiver;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

pub type StateChannel = Arc<RwLock<SharedState>>;
pub type StopChannel = Receiver<()>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ControlMode {
    Tracking,
    Manual,
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde_as(as = "serde_with::DurationNanoSeconds<u64>")]
    pub cycle_time_ns: Duration,
    pub serial_device: String,
    pub opcua_config_path: PathBuf,
    pub control_mode: ControlMode,
    pub limit_max_coax: u32,
    pub limit_min_coax: u32,
    pub maxspeed_coax: u32,
    pub accel_coax: u32,
    pub offset_coax: i32,
    pub limit_max_cross: u32,
    pub limit_min_cross: u32,
    pub maxspeed_cross: u32,
    pub accel_cross: u32,
    pub mock_zaber: bool,
    pub formula_coax: String,
    pub formula_cross: String,
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
    pub target_coax: u32,
    pub target_cross: u32,
    pub position_cross: u32,
    pub position_coax: u32,
    pub voltage: [f64; 2],
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
    pub target_manual: Arc<RwLock<(u32, u32, f64, f64)>>,
    pub config: Arc<RwLock<Config>>,
}
