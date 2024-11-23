use std::{cell::RefCell, rc::Rc, sync::{Arc, RwLock}};

use anyhow::{Result, anyhow};
use zproto::{
    ascii::{
        response::{check, Status},
        Port,
    },
    backend::Backend,
};
use crate::control::{self, run, ExecState};

pub const MICROSTEP_SIZE: f64 = 0.49609375; //µm
pub const MAX_POS: usize = 201574; // microsteps

type ZaberConn<T> = Port<'static, T>;

pub fn init_zaber(state: &mut ExecState) -> Result<()> {
    let (serial_device, backend) = {
        let s = state.config.read().unwrap();
        (s.serial_device.clone(), s.backend.clone())
    };

    let mut zaber_conn = match Port::open_serial(&serial_device) {
        Ok(zaber_conn) => zaber_conn,
        Err(e) => return Err(anyhow!("Failed to open Zaber serial port '{}': {}", serial_device, e)),
    };

    zaber_conn.command_reply_n("system restore", 2, check::unchecked())?;

    let _ = zaber_conn.command_reply_n("home", 2, check::flag_ok());

    zaber_conn.poll_until_idle(1, check::flag_ok())?;
    zaber_conn.poll_until_idle(2, check::flag_ok())?;

    zaber_conn.command_reply_n("set comm.alert 0", 2, check::flag_ok())?;

    zaber_conn.command_reply((1, "lockstep 1 setup enable 1 2"))?.flag_ok()?;

    let zaber_conn = Rc::new(RefCell::new(zaber_conn));

    let get_pos = || get_pos_zaber(Rc::clone(&zaber_conn));
    let move_parallel = |pos| move_parallel_zaber(Rc::clone(&zaber_conn), pos);
    let move_cross = |pos| move_cross_zaber(Rc::clone(&zaber_conn), pos);

    match backend {
        control::Backend::Manual => {
            let voltage_shared = Arc::clone(&state.voltage_manual);
            let voltage  = Rc::new(RefCell::new(0.));
            let get_voltage = move || get_voltage_manual(Rc::clone(&voltage), Arc::clone(&voltage_shared));
            return run(state, get_voltage, get_pos, move_parallel, move_cross);
        },
        _ => {
            let get_voltage = || get_voltage_zaber(Rc::clone(&zaber_conn));
            return run(state, get_voltage, get_pos, move_parallel, move_cross);
        },
    };
}

pub fn get_voltage_zaber<T: Backend>(zaber_conn: Rc<RefCell<ZaberConn<T>>>) -> Result<f64> {
    let cmd = format!("io get ai 1");
    let reply = zaber_conn.borrow_mut().command_reply((1, cmd))?.flag_ok()?;
    return Ok(reply.data().parse()?);
}

fn get_voltage_manual(voltage: Rc<RefCell<f64>>, voltage_shared: Arc<RwLock<f64>>) -> Result<f64> {
    let ref mut voltage = *voltage.borrow_mut(); 
    let Ok(shared) = voltage_shared.try_read() else {
        return Ok(*voltage);
    };
    *voltage = *shared;

    return Ok(*shared);
}

pub fn get_pos_zaber<T: Backend>(zaber_conn: Rc<RefCell<ZaberConn<T>>>) -> Result<(f64, f64, bool, bool)> {
    let mut pos_parallel = 0.;
    let mut busy_parallel = false;
    let mut pos_cross = 0.;
    let mut busy_cross = false;
    for reply in zaber_conn.borrow_mut().command_reply_n_iter("get pos", 2)? {
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
    return Ok((pos_parallel * MICROSTEP_SIZE, pos_cross * MICROSTEP_SIZE, busy_parallel, busy_cross));
}

pub fn move_parallel_zaber<T: Backend>(zaber_conn: Rc<RefCell<ZaberConn<T>>>, pos: f64) -> Result<()> {
    let cmd = format!("lockstep 1 move abs {}", (pos / MICROSTEP_SIZE) as u64);
    let _ = zaber_conn.borrow_mut().command_reply((1, cmd))?.flag_ok()?;
    Ok(())
}

pub fn move_cross_zaber<T: Backend>(zaber_conn: Rc<RefCell<ZaberConn<T>>>, pos: f64) -> Result<()> {
    let cmd = format!("move abs {}", (pos / MICROSTEP_SIZE) as u64);
    let _ = zaber_conn.borrow_mut().command_reply((2, cmd))?.flag_ok()?;
    Ok(())
}
