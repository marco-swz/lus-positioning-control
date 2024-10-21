use anyhow::{anyhow, Result};
use std::{
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use crossbeam_queue::ArrayQueue;
use zproto::{
    ascii::{
        response::{check, Status},
        Port,
    },
    backend::Backend,
};

type ZaberConn<T> = Port<'static, T>;
pub type StateChannel = Arc<ArrayQueue<SharedState>>;
pub type StopChannel = Arc<(Mutex<bool>, Condvar)>;

#[derive(Clone, Debug)]
pub struct Config {
    pub cycle_time_ms: u64,
    pub voltage_min: f64,
    pub voltage_max: f64,
    pub serial_device: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlState {
    Disconnected,
    Connected,
    Init,
    Running,
    Reset,
    Stopped,
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
    pub stop_channel: StopChannel,
    pub config: Config,
}

pub fn connect(state: &mut ExecState) -> Result<()> {
    state.shared.control_state = ControlState::Connected;
    state.out_channel.force_push(state.shared.clone());

    let mut zaber_conn = Port::open_serial(&state.config.serial_device)?;

    return init(state, &mut zaber_conn);
}

fn init<T: Backend>(state: &mut ExecState, zaber_conn: &mut ZaberConn<T>) -> Result<()> {
    state.shared.control_state = ControlState::Init;
    state.out_channel.force_push(state.shared.clone());

    zaber_conn.command_reply((1, "lockstep 1 setup disable"));

    let _ = zaber_conn.command_reply_n("home", 2, check::flag_ok());

    zaber_conn.poll_until_idle(1, check::flag_ok())?;
    println!("cp");
    //zaber_conn.poll_until_idle(2, check::flag_ok());

    println!("cp1");

    zaber_conn.command_reply_n("set comm.alert 0", 2, check::flag_ok())?;

    println!("cp2");

    zaber_conn.command_reply((1, "lockstep 1 setup enable 1 2"));

    println!("cp3");

    return run(state, zaber_conn);
}

fn run<T: Backend>(state: &mut ExecState, zaber_conn: &mut ZaberConn<T>) -> Result<()> {
    state.shared.control_state = ControlState::Running;

    let voltage_max = state.config.voltage_max;
    let voltage_min = state.config.voltage_min;

    println!("cp2");
    let (lock, cvar) = &*state.stop_channel;
    //let mut stop = lock.lock().unwrap();

    loop {
        let cmd = format!("io get ai 1");
        let reply = zaber_conn.command_reply((1, cmd))?.flag_ok()?;
        let voltage_gleeble = reply.data().parse()?;
        state.shared.voltage_gleeble = voltage_gleeble;

        let target_position_parallel = voltage_gleeble - voltage_min / (voltage_max - voltage_min);

        println!("cp");
        let cmd = format!("lockstep 1 move abs {}", target_position_parallel as u64);
        let _ = zaber_conn.command_reply((1, cmd))?.flag_ok()?;

        println!("cp-");

        let cmd = format!("get pos");
        for reply in zaber_conn.command_reply_n_iter(cmd, 2)? {
            let reply = reply?.check(check::unchecked())?;
            match reply.target().device() {
                1 => {
                    state.shared.position_parallel = reply
                        .data()
                        .split_whitespace()
                        .next()
                        .ok_or(anyhow!(""))?
                        .parse()?;
                    state.shared.busy_parallel = if reply.status() == Status::Busy {
                        true
                    } else {
                        false
                    };
                }
                2 => {
                    state.shared.position_cross = reply.data().parse()?;
                    state.shared.busy_cross = if reply.status() == Status::Busy {
                        true
                    } else {
                        false
                    };
                }
                _ => {
                    return Err(anyhow!(
                        "Unkown device with number {}",
                        reply.target().device()
                    ))
                }
            }
        }

        state.out_channel.force_push(state.shared.clone());

        std::thread::sleep(Duration::from_millis(1000));

        //let result = cvar
        //    .wait_timeout(stop, Duration::from_millis(state.config.cycle_time_ms))
        //    .unwrap();
        //stop = result.0;
        //if *stop {
        //    break;
        //}
    }

    state.shared.control_state = ControlState::Stopped;
    state.out_channel.push(state.shared.clone());
    return Ok(());
}

#[cfg(test)]
mod tests {
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
        // /io get ai 1
        backend.push(b"@01 0 OK BUSY -- 5.5\r\n");
        // /move abs
        backend.push(b"@01 0 OK BUSY -- 0\r\n");
        // /get pos
        backend.push(b"@01 0 OK BUSY -- 20\r\n");
        backend.push(b"@02 0 OK IDLE -- 10.1\r\n");

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
}
