use std::sync::Arc;

use crossbeam_queue::ArrayQueue;
use zproto::{
    ascii::{response::Status, Port},
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

#[derive(Clone)]
pub struct ZaberState {
    pub position_cross: f64,
    pub position_parallel: f64,
    pub busy_cross: bool,
    pub busy_parallel: bool,
    pub control_state: ControlState,
}

pub fn connect_state(state_queue: StateQueue) {
    let zaber_state = ZaberState{
        position_cross: 0.,
        position_parallel: 0.,
        busy_cross: false,
        busy_parallel: false,
        control_state: ControlState::Connect,
    };
    state_queue.force_push(zaber_state);

    loop {
        match Port::open_serial("/dev/ttyACM0") {
            Ok(z) => {
                let _ = init_state(z, Arc::clone(&state_queue)); 
            },
            Err(e) => {
                println!("{}", e);
                
            },
        };

        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}

fn init_state<T: Backend>(mut zaber_conn: ZaberConn<T>, state_queue: StateQueue) -> ZaberConn<T> {
    let zaber_state = ZaberState{
        position_cross: 0.,
        position_parallel: 0.,
        busy_cross: false,
        busy_parallel: false,
        control_state: ControlState::Init,
    };
    state_queue.force_push(zaber_state);

    let cmd = format!("home");
    let Ok(_) = zaber_conn.command_reply((0, cmd)) else {
        return reset_state(zaber_conn, state_queue);
    };

    return run_state(zaber_conn, state_queue)

}

fn run_state<T: Backend>(mut zaber_conn: ZaberConn<T>, state_queue: StateQueue) -> ZaberConn<T> {

    let mut voltage_gleeble = 10.;
    let max = 100.;
    let min = 5.;
    let vel = 5.;
    let acc = 5.;

    loop {
        voltage_gleeble += 1.;
        let position_gleeble = voltage_gleeble - min / (max - min);

        let cmd = format!("move abs {} {} {}", position_gleeble, vel, acc);
        let Ok(_) = zaber_conn.command_reply((0, cmd)) else {
            return reset_state(zaber_conn, state_queue);
        };

        let zaber_state = ZaberState{
            position_cross: position_gleeble,
            position_parallel: 0.,
            busy_cross: false,
            busy_parallel: false,
            control_state: ControlState::Run,
        };
        state_queue.force_push(zaber_state);


        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn reset_state<T: Backend>(zaber_conn: ZaberConn<T>, state_queue: StateQueue) -> ZaberConn<T> {
    let zaber_state = ZaberState{
        position_cross: 0.,
        position_parallel: 0.,
        busy_cross: false,
        busy_parallel: false,
        control_state: ControlState::Reset,
    };
    state_queue.force_push(zaber_state);
    return zaber_conn;
}

#[cfg(test)]
mod tests {
    use super::*;
    use zproto::backend::Mock;
    use std::io::Read;

    #[test]
    fn test_run_state() {
        let mut port = Port::open_mock();
        let backend = port.backend_mut();
        backend.push(b"@01 0 OK IDLE -- 0\r\n");

        let queue = Arc::new(ArrayQueue::new(1));

        let zaber_state = ZaberState {
            position_cross: 0.,
            position_parallel: 0.,
            busy_cross: false,
            busy_parallel: false,
            control_state: ControlState::PreConnect,
        };

        let _ = queue.force_push(zaber_state);

        let mut port = run_state(port, queue);
        let mut buf = Vec::new();
        port.backend_mut().read(&mut buf).unwrap();

        dbg!(&buf);
        assert!(true);
    }

}
