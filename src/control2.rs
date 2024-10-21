use std::sync::Arc;

use crossbeam_queue::ArrayQueue;
use zproto::{
    ascii::{Port, SendPort, Status},
    backend::Serial,
};

type ZaberConn = SendPort<'static, Serial>;
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
            Ok(z) => init_state(z.try_into_send().unwrap(), Arc::clone(&state_queue)),
            Err(e) => {
                println!("{}", e);
                
            },
        };

        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}

fn init_state(mut zaber_conn: ZaberConn, state_queue: StateQueue) {
    let zaber_state = ZaberState{
        position_cross: 0.,
        position_parallel: 0.,
        busy_cross: false,
        busy_parallel: false,
        control_state: ControlState::Init,
    };
    state_queue.force_push(zaber_state);

    // TODO(marco): Empty the serial buffer
    match zaber_conn.command_reply((1, "home")) {
        Ok(i) => {
            println!("home: {}", i);
        },
        Err(e) => {
            println!("home: {}", e);
            //return reset_state(zaber_conn, state_queue);
        }
    };

    match zaber_conn.poll_until_idle(1) {
        Ok(_) => (),
        Err(e) => println!("{}", e),
    }

    // Disable alerts
    match zaber_conn.command_reply_n((1, "set comm.alert 0"), 1) {
        Ok(i) => {
            for msg in i.iter() {
                println!("comm: {}", msg);
            }
        },
        Err(e) => {
            println!("comm: {}", e);
        }
    };

    match zaber_conn.command_reply((1, "lockstep 1 setup enable 1 2")) {
        Ok(i) => {
            println!("lockstep: {}", i);
        },
        Err(e) => {
            println!("lockstep: {}", e);
            //return reset_state(zaber_conn, state_queue);
        }
    };

    //match zaber_conn.command_reply((1, "lockstep 1 home")) {
    //    Ok(i) => {
    //        println!("home: {}", i);
    //    },
    //    Err(e) => {
    //        println!("home: {}", e);
    //        //return reset_state(zaber_conn, state_queue);
    //    }
    //};

    /*
    match zaber_conn.command_reply((2, "home")) {
        Ok(i) => {
            println!("home2: {}", i);
        },
        Err(e) => {
            println!("home2: {}", e);
            //return reset_state(zaber_conn, state_queue);
        }
    };
    */

    return run_state(zaber_conn, state_queue)
}

fn run_state(mut zaber_conn: ZaberConn, state_queue: StateQueue) {

    let mut voltage_gleeble = 10.;
    let max = 100.;
    let min = 5.;
    let vel = 5.;
    let acc = 5.;

    loop {
        voltage_gleeble += 1.;
        let position_gleeble = voltage_gleeble - min / (max - min);

        //let cmd = format!("move rel {} {} {}", position_gleeble, vel, acc);
        match zaber_conn.command_reply_infos((1, "get pos")) {
            Ok(r) => println!("{:?}", r),
            Err(e) => {
                println!("pos: {}", e);
                return reset_state(zaber_conn, state_queue);
            }
        };
        
        let cmd = format!("lockstep 1 move abs {}", position_gleeble);
        match zaber_conn.command_reply((1, cmd)) {
            Ok(_) => (),
            Err(e) => {
                println!("{}", e);
                return reset_state(zaber_conn, state_queue);
            }
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

fn reset_state(mut zaber_conn: ZaberConn, state_queue: StateQueue) {
    let zaber_state = ZaberState{
        position_cross: 0.,
        position_parallel: 0.,
        busy_cross: false,
        busy_parallel: false,
        control_state: ControlState::Reset,
    };
    state_queue.force_push(zaber_state);

    let _ = zaber_conn.command("stop");
    let _ = zaber_conn.command("lockstep 1 setup disable");
}
