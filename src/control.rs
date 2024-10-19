use std::{sync::{Arc, Condvar, Mutex, MutexGuard}, time::Duration};

use crossbeam_queue::ArrayQueue;
use zproto::{
    ascii::{command::Command, response::{Response, Status}, Port},
    backend::{Backend, Serial},
};

type ZaberConn<T> = Port<'static, T>;
pub type StateChannel = Arc<ArrayQueue<SharedState>>;
pub type StopChannel = Arc<(Mutex<bool>, Condvar)>;

#[derive(Clone, Debug)]
pub struct Config {
    pub cycle_time_ms: u64,
    pub voltage_min: f64,
    pub voltage_max: f64,
}

#[derive(Clone, Debug)]
pub enum ControlState {
    PreConnect,
    Connect,
    Init,
    Run,
    Reset,
}

#[derive(Clone, Debug)]
pub struct SharedState {
    pub voltage_gleeble: f64,
    pub position_cross: f64,
    pub position_parallel: f64,
    pub busy_cross: bool,
    pub busy_parallel: bool,
    pub control_state: ControlState,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ExecState {
    pub shared: SharedState,
    pub out_channel: StateChannel,
    pub stop_channel: StopChannel,
    pub config: Config, 
}


pub fn connect_state(mut state: ExecState) {

    loop {
        state.shared.control_state = ControlState::Connect;
        state.out_channel.force_push(state.shared.clone());

        match Port::open_serial("/dev/ttyACM0") {
            Ok(z) => {
                (state, _) = init_state(state, z); 
            },
            Err(e) => {
                println!("{}", e);
                
            },
        };

        let (lock, cvar) = &*state.stop_channel;
        let mut stop = lock.lock().unwrap();
        while !*stop {
            let result = cvar.wait_timeout(stop, Duration::from_secs(5)).unwrap();
            stop = result.0;
        }
        drop(stop);
    }
}

fn init_state<T: Backend>(mut state: ExecState, mut zaber_conn: ZaberConn<T>) -> (ExecState, ZaberConn<T>) {
    state.shared.control_state = ControlState::Init;
    state.out_channel.force_push(state.shared.clone());

    let cmd = format!("home");
    let Ok(_) = zaber_conn.command_reply((0, cmd)) else {
        return reset_state(state, zaber_conn);
    };

    return run_state(state, zaber_conn);

}

fn run_state<T: Backend>(mut state: ExecState, mut zaber_conn: ZaberConn<T>) -> (ExecState, ZaberConn<T>) {
    state.shared.control_state = ControlState::Run;

    let voltage_max = state.config.voltage_max;
    let voltage_min = state.config.voltage_min;

    let (lock, cvar) = &*state.stop_channel;
    let mut stop = lock.lock().unwrap();
    loop {

        let cmd = format!("io get ai 1");
        let voltage_gleeble = match zaber_conn.command_reply((0, cmd)) {
            Ok(reply) => match reply.flag_ok() {
                Ok(val) => val.data().parse().unwrap_or(0.),
                Err(e) => {
                    state.shared.error = Some(e.to_string());
                    break;
                }
            }
            Err(e) => {
                state.shared.error = Some(e.to_string());
                break;
            }
        };
        state.shared.voltage_gleeble = voltage_gleeble;

        let target_position_parallel = voltage_gleeble - voltage_min / (voltage_max - voltage_min);


        let cmd = format!("move abs {}", target_position_parallel);
        let Ok(_) = zaber_conn.command_reply((0, cmd)) else {
            break;
        };

        let cmd = format!("get pos");
        let (pos_parallel, pos_cross) = match zaber_conn.command_reply(cmd) {
            Ok(reply) => match reply.flag_ok() {
                Ok(val) => {
                    let mut values = val.data().split_whitespace();
                    let pos1: f64 = values.next().unwrap_or("error").parse().unwrap_or(-1.);
                    let pos2: f64 = values.next().unwrap_or("error").parse().unwrap_or(-1.);
                    (pos1, pos2)
                }
                Err(e) => {
                    state.shared.error = Some(e.to_string());
                    break;
                }
            }
            Err(e) => {
                state.shared.error = Some(e.to_string());
                break;
            }
        };
        state.shared.position_cross = pos_cross;
        state.shared.position_parallel = pos_parallel;

        state.out_channel.force_push(state.shared.clone());

        let result = cvar.wait_timeout(stop, Duration::from_millis(state.config.cycle_time_ms)).unwrap();
        stop = result.0;
        if *stop {
            break;
        }
    }

    drop(stop);
    return reset_state(state, zaber_conn);
}

fn reset_state<T: Backend>(mut state: ExecState, zaber_conn: ZaberConn<T>) -> (ExecState, ZaberConn<T>) {
    state.shared.control_state = ControlState::Reset;
    state.out_channel.force_push(state.shared.clone());
    return (state, zaber_conn);
}

#[cfg(test)]
mod tests {
    use super::*;
    //use zproto::backend::Mock;
    use std::io::Read;

    #[test]
    fn test_run_state() {
        let mut port = Port::open_mock();
        let backend = port.backend_mut();
        // /io get ai 1
        backend.push(b"@01 0 OK BUSY -- 5.5\r\n");
        // /move abs
        backend.push(b"@01 0 OK BUSY -- 0\r\n");
        // /get pos
        backend.push(b"@01 0 OK BUSY -- 20 10.1\r\n");
        backend.push(b"@02 0 OK BUSY -- 10.1\r\n");


        let state = ExecState{
            shared: SharedState {
                voltage_gleeble: 0.,
                position_cross: 0.,
                position_parallel: 0.,
                busy_cross: false,
                busy_parallel: false,
                control_state: ControlState::PreConnect,
                error: None,
            },
            config: Config{
                cycle_time_ms: 1000,
                voltage_min: 5.,
                voltage_max: 100.,
            },
            out_channel: Arc::new(ArrayQueue::new(1)),
            stop_channel: Arc::new((Mutex::new(false), Condvar::new())),
        };

        let _ = state.out_channel.force_push(state.shared.clone());

        let (state, mut port) = run_state(state, port);
        dbg!(&state);
        let mut buf = Vec::new();
        port.backend_mut().read(&mut buf).unwrap();

        dbg!(&buf);
        assert!(true);
    }

}
