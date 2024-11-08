use std::{cell::RefCell, rc::Rc};

use anyhow::{Result, anyhow};
use zproto::{
    ascii::{
        response::{check, Status},
        Port,
    },
    backend::Backend,
};
use crate::control::{run, ControlState, ExecState};

type ZaberConn<T> = Port<'static, T>;

pub fn init_zaber(state: &mut ExecState) -> Result<()> {
    let mut zaber_conn = Port::open_serial(&state.config.serial_device)?;

    state.shared.control_state = ControlState::Init;
    let mut out = state.out_channel.write().unwrap();
    *out = state.shared.clone();
    drop(out);

    zaber_conn.command_reply((1, "lockstep 1 setup disable"))?.check(check::unchecked())?;

    let _ = zaber_conn.command_reply_n("home", 2, check::flag_ok());

    zaber_conn.poll_until_idle(1, check::flag_ok())?;
    println!("cp");
    //zaber_conn.poll_until_idle(2, check::flag_ok());

    println!("cp1");

    zaber_conn.command_reply_n("set comm.alert 0", 2, check::flag_ok())?;

    println!("cp2");

    zaber_conn.command_reply((1, "lockstep 1 setup enable 1 2"))?.flag_ok()?;

    println!("cp3");
    let zaber_conn = Rc::new(RefCell::new(zaber_conn));

    let get_voltage = || get_voltage_zaber(Rc::clone(&zaber_conn));
    let get_pos = || get_pos_zaber(Rc::clone(&zaber_conn));
    let move_parallel = |pos| move_parallel_zaber(Rc::clone(&zaber_conn), pos);
    let move_cross = |pos| move_cross_zaber(Rc::clone(&zaber_conn), pos);

    return run(state, get_voltage, get_pos, move_parallel, move_cross);
}

pub fn get_voltage_zaber<T: Backend>(zaber_conn: Rc<RefCell<ZaberConn<T>>>) -> Result<f64> {
    let cmd = format!("io get ai 1");
    let reply = zaber_conn.borrow_mut().command_reply((1, cmd))?.flag_ok()?;
    return Ok(reply.data().parse()?);
}

pub fn get_pos_zaber<T: Backend>(zaber_conn: Rc<RefCell<ZaberConn<T>>>) -> Result<(f64, f64, bool, bool)> {
    let cmd = format!("get pos");
    let mut pos_parallel = 0.;
    let mut busy_parallel = false;
    let mut pos_cross = 0.;
    let mut busy_cross = false;
    for reply in zaber_conn.borrow_mut().command_reply_n_iter(cmd, 2)? {
        let reply = reply?.check(check::unchecked())?;
        match reply.target().device() {
            1 => {
                pos_parallel = reply
                    .data()
                    .split_whitespace()
                    .next()
                    .ok_or(anyhow!(""))?
                    .parse()?;
                busy_parallel = if reply.status() == Status::Busy {
                    true
                } else {
                    false
                };
            }
            2 => {
                pos_cross = reply.data().parse()?;
                busy_cross = if reply.status() == Status::Busy {
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
    return Ok((pos_parallel, pos_cross, busy_parallel, busy_cross));
}

pub fn move_parallel_zaber<T: Backend>(zaber_conn: Rc<RefCell<ZaberConn<T>>>, pos: f64) -> Result<()> {
    let cmd = format!("lockstep 1 move abs {}", pos as u64);
    let _ = zaber_conn.borrow_mut().command_reply((1, cmd))?.flag_ok()?;
    Ok(())
}

pub fn move_cross_zaber<T: Backend>(zaber_conn: Rc<RefCell<ZaberConn<T>>>, pos: f64) -> Result<()> {
    let cmd = format!("move abs {}", pos as u64);
    let _ = zaber_conn.borrow_mut().command_reply((2, cmd))?.flag_ok()?;
    Ok(())
}
