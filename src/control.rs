use std::sync::Arc;

use crossbeam_queue::ArrayQueue;
use zproto::{
    ascii::{command::Command, response::{Response, Status}, Port},
    backend::{Backend, Serial},
};

type ZaberConn<T> = Port<'static, T>;
pub type StateQueue = Arc<ArrayQueue<ZaberState>>;

#[derive(Clone, Debug)]
pub enum ControlState {
    PreConnect,
    Connect,
    Init,
    Run,
    Reset,
}

#[derive(Clone, Debug)]
pub struct ZaberState {
    pub voltage_gleeble: f64,
    pub position_cross: f64,
    pub position_parallel: f64,
    pub busy_cross: bool,
    pub busy_parallel: bool,
    pub control_state: ControlState,
    pub error: Option<String>,
}

pub fn connect_state(state_queue: StateQueue, mut state: ZaberState) {
    state.control_state = ControlState::Connect;
    state_queue.force_push(state.clone());

    loop {
        match Port::open_serial("/dev/ttyACM0") {
            Ok(z) => {
                (_, state) = init_state(z, state, Arc::clone(&state_queue)); 
            },
            Err(e) => {
                println!("{}", e);
                
            },
        };

        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}

fn init_state<T: Backend>(mut zaber_conn: ZaberConn<T>, mut state: ZaberState, state_queue: StateQueue) -> (ZaberConn<T>, ZaberState) {
    state.control_state = ControlState::Init;
    state_queue.force_push(state.clone());

    let cmd = format!("home");
    let Ok(_) = zaber_conn.command_reply((0, cmd)) else {
        return reset_state(zaber_conn, state, state_queue);
    };

    return run_state(zaber_conn, state, state_queue)

}

fn run_state<T: Backend>(mut zaber_conn: ZaberConn<T>, mut state: ZaberState, state_queue: StateQueue) -> (ZaberConn<T>, ZaberState) {
    state.control_state = ControlState::Run;

    let max = 100.;
    let min = 5.;

    loop {

        let cmd = format!("io get ai 1");
        let voltage_gleeble = match zaber_conn.command_reply((0, cmd)) {
            Ok(reply) => match reply.flag_ok() {
                Ok(val) => val.data().parse().unwrap_or(0.),
                Err(e) => {
                    state.error = Some(e.to_string());
                    return reset_state(zaber_conn, state, state_queue);
                }
            }
            Err(e) => {
                state.error = Some(e.to_string());
                return reset_state(zaber_conn, state, state_queue);
            }
        };
        state.voltage_gleeble = voltage_gleeble;

        let target_position_parallel = voltage_gleeble - min / (max - min);


        let cmd = format!("move abs {}", target_position_parallel);
        let Ok(_) = zaber_conn.command_reply((0, cmd)) else {
            return reset_state(zaber_conn, state, state_queue);
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
                    state.error = Some(e.to_string());
                    return reset_state(zaber_conn, state, state_queue);
                }
            }
            Err(e) => {
                state.error = Some(e.to_string());
                return reset_state(zaber_conn, state, state_queue);
            }
        };
        state.position_cross = pos_cross;
        state.position_parallel = pos_parallel;

        state_queue.force_push(state.clone());


        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn reset_state<T: Backend>(zaber_conn: ZaberConn<T>, mut state: ZaberState, state_queue: StateQueue) -> (ZaberConn<T>, ZaberState) {
    state.control_state = ControlState::Reset;
    state_queue.force_push(state.clone());
    return (zaber_conn, state);
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

        let queue = Arc::new(ArrayQueue::new(1));

        let state = ZaberState {
            voltage_gleeble: 0.,
            position_cross: 0.,
            position_parallel: 0.,
            busy_cross: false,
            busy_parallel: false,
            control_state: ControlState::PreConnect,
            error: None,
        };

        let _ = queue.force_push(state.clone());

        let (mut port, state) = run_state(port, state, queue);
        dbg!(&state);
        let mut buf = Vec::new();
        port.backend_mut().read(&mut buf).unwrap();

        dbg!(&buf);
        assert!(true);
    }

}
