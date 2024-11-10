use anyhow::Result;
use crossbeam_channel::Receiver;
use std::{
    fmt::Display, sync::{Arc, RwLock}, time::Duration
};

use crate::zaber::init_zaber;

pub type StateChannel = Arc<RwLock<SharedState>>;
pub type StopChannel = Receiver<()>;

#[derive(Clone, Debug)]
pub struct Config {
    pub cycle_time: Duration,
    pub restart_timeout: Duration,
    pub voltage_min: f64,
    pub voltage_max: f64,
    pub serial_device: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlState {
    Disconnected,
    Init,
    Running,
    Stopped,
}

impl Display for ControlState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::Disconnected => "Disconnected",
            Self::Init => "Init",
            Self::Running => "Running",
            Self::Stopped => "Stopped",
        };
        write!(f, "{}", text)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SharedState {
    pub voltage_gleeble: f64,
    pub position_cross: f64,
    pub position_parallel: f64,
    pub busy_cross: bool,
    pub busy_parallel: bool,
    pub control_state: ControlState,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct ExecState {
    pub shared: SharedState,
    pub out_channel: StateChannel,
    pub rx_stop: StopChannel,
    pub config: Config,
}

pub fn init(state: &mut ExecState) -> Result<()> {
    return init_zaber(state);
}

pub fn run(
    state: &mut ExecState,
    mut get_voltage: impl FnMut() -> Result<f64>,
    mut get_pos: impl FnMut() -> Result<(f64, f64, bool, bool)>,
    mut move_parallel: impl FnMut(f64) -> Result<()>,
    _move_cross: impl FnMut(f64) -> Result<()>,
) -> Result<()> {
    state.shared.control_state = ControlState::Running;

    let voltage_max = state.config.voltage_max;
    let voltage_min = state.config.voltage_min;

    loop {
        let voltage_gleeble = get_voltage()?;
        state.shared.voltage_gleeble = voltage_gleeble;

        let target_position_parallel = voltage_gleeble - voltage_min / (voltage_max - voltage_min);

        move_parallel(target_position_parallel)?;

        let (pos_parallel, pos_cross, busy_parallel, busy_cross) = get_pos()?;
        state.shared.position_parallel = pos_parallel;
        state.shared.position_cross = pos_cross;
        state.shared.busy_parallel = busy_parallel;
        state.shared.busy_cross = busy_cross;

        if let Ok(mut out) = state.out_channel.try_write() {
            *out = state.shared.clone();
            drop(out);
        }

        if let Ok(_) = state.rx_stop.recv_timeout(state.config.cycle_time) {
            break;
        }
    }

    state.shared.control_state = ControlState::Stopped;
    let mut out = state.out_channel.write().unwrap();
    *out = state.shared.clone();
    drop(out);
    return Ok(());
}

#[cfg(test)]
mod tests {
    /*
    use zproto::ascii::Port;

    use super::*;


    fn prepare_state() -> ExecState {
        let state = ExecState {
            shared: SharedState {
                voltage_gleeble: 0.,
                position_cross: 0.,
                position_parallel: 0.,
                busy_cross: false,
                busy_parallel: false,
                control_state: ControlState::Init,
                error: None,
            },
            config: Config {
                cycle_time_ms: 1000,
                voltage_min: 5.,
                voltage_max: 100.,
                serial_device: "".to_string(),
            },
            out_channel: Arc::new(ArrayQueue::new(1)),
            stop_channel: Arc::new((Mutex::new(false), Condvar::new())),
        };

        return state;
    }

    #[test]
    /// Single loop with stop command
    fn test_run_stop() {
        let mut port = Port::open_mock();
        let backend = port.backend_mut();
        backend.set_reply_callback(|buffer, msg| {
            buffer.extend_from_slice(match msg {
                b"/1 io get ai 1\n" => b"@01 0 OK BUSY -- 5.5\r\n",
                b"/1 lockstep 1 move abs 5\n" => b"@01 0 OK BUSY -- 0\r\n",
                b"/get pos\n" => b"@01 0 OK BUSY -- 20\r\n@02 0 OK IDLE -- 10.1\r\n",
                e => panic!("unexpected message: {:?}", e),
            })
        });

        //// /io get ai 1
        //backend.push(b"@01 0 OK BUSY -- 5.5\r\n");
        //// /move abs
        //backend.push(b"@01 0 OK BUSY -- 0\r\n");
        //// /get pos
        //backend.push(b"@01 0 OK BUSY -- 20\r\n");
        //backend.push(b"@02 0 OK IDLE -- 10.1\r\n");

        let mut state = prepare_state();
        let _ = state.out_channel.force_push(state.shared.clone());

        {
            let (lock, cvar) = &*state.stop_channel;
            let mut stop = lock.lock().unwrap();
            *stop = true;
            cvar.notify_one();
        }

        assert!(run(&mut state, &mut port).is_ok());

        assert_eq!(
            state.shared,
            SharedState {
                voltage_gleeble: 5.5,
                position_parallel: 20.,
                position_cross: 10.1,
                busy_parallel: true,
                busy_cross: false,
                control_state: ControlState::Stopped,
                error: None,
            }
        );
    }

    #[test]
    /// Two loops with timeout (= disconnect)
    fn test_run_disconnect() {
        let mut port = Port::open_mock();
        let backend = port.backend_mut();
        // /io get ai 1
        backend.push(b"@01 0 OK BUSY -- 5.5\r\n");
        // /move abs
        backend.push(b"@01 0 OK BUSY -- 0\r\n");
        // /get pos
        backend.push(b"@01 0 OK BUSY -- 20\r\n");
        backend.push(b"@02 0 OK IDLE -- 10.1\r\n");

        // /io get ai 1
        backend.push(b"@01 0 OK BUSY -- 6.5\r\n");
        // /move abs
        backend.push(b"@01 0 OK BUSY -- 0\r\n");
        // /get pos
        backend.push(b"@01 0 OK IDLE -- 20.1\r\n");
        // Last message missing -> timeout

        let mut state = prepare_state();
        let _ = state.out_channel.force_push(state.shared.clone());

        assert!(run(&mut state, &mut port).is_err());

        assert_eq!(
            state.shared,
            SharedState {
                voltage_gleeble: 6.5,
                position_parallel: 20.1,
                position_cross: 10.1,
                busy_parallel: false,
                busy_cross: false,
                control_state: ControlState::Running,
                error: None,
            }
        );
    }

    #[test]
    /// Error in reply
    fn test_run_reply() {
        let mut port = Port::open_mock();
        let backend = port.backend_mut();
        // /io get ai 1
        backend.push(b"@01 0 OK BUSY -- 5.5\r\n");
        // /move abs
        backend.push(b"@01 0 OK BUSY -- 0\r\n");
        // /get pos
        backend.push(b"@01 0 OK BUSY -- 20\r\n");
        backend.push(b"@02 0 OK IDLE -- 10.1\r\n");

        // /io get ai 1
        backend.push(b"@01 0 OK BUSY -- 6.5\r\n");
        // /move abs
        backend.push(b"@01 0 OK BUSY -- 0\r\n");
        // /get pos
        backend.push(b"@01 0 OK IDLE -- 20.1\r\n");
        // Last message missing -> timeout

        let mut state = prepare_state();
        let _ = state.out_channel.force_push(state.shared.clone());

        assert!(run(&mut state, &mut port).is_err());

        assert_eq!(
            state.shared,
            SharedState {
                voltage_gleeble: 6.5,
                position_parallel: 20.1,
                position_cross: 10.1,
                busy_parallel: false,
                busy_cross: false,
                control_state: ControlState::Running,
                error: None,
            }
        );
    }

    #[test]
    /// Error in reply
    fn test_run_reply_err() {
        let mut port = Port::open_mock();
        let backend = port.backend_mut();
        // /io get ai 1
        backend.push(b"@01 0 OK BUSY -- 5.5\r\n");
        // /move abs
        backend.push(b"@01 0 RJ BUSY WR 0\r\n");
        // /get pos
        backend.push(b"@01 0 OK BUSY -- 20\r\n");
        backend.push(b"@02 0 OK IDLE -- 10.1\r\n");

        let mut state = prepare_state();
        let _ = state.out_channel.force_push(state.shared.clone());

        assert!(run(&mut state, &mut port).is_err());

        assert_eq!(
            state.shared,
            SharedState {
                voltage_gleeble: 5.5,
                position_parallel: 0.,
                position_cross: 0.,
                busy_parallel: false,
                busy_cross: false,
                control_state: ControlState::Running,
                error: None,
            }
        );
    }
    */
}
