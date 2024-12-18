use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, RwLock},
};

use crate::{
    control::run,
    utils::{self, ExecState},
};
use anyhow::{anyhow, Result};
use zproto::{
    ascii::{
        response::{check, Status},
        Port,
    },
    backend::Backend,
};

pub const MICROSTEP_SIZE: f64 = 0.49609375; //µm
pub const VELOCITY_FACTOR: f64 = 1.6384;
pub const MAX_POS: u64 = 201574; // microsteps
pub const MAX_SPEED: u64 = 153600; // microsteps/sec

type ZaberConn<T> = Port<'static, T>;

pub fn init_zaber(state: &mut ExecState) -> Result<()> {
    let config = {
        let s = state.config.read().unwrap();
        s.clone()
    };

    let mut zaber_conn = match Port::open_serial(&config.serial_device) {
        Ok(zaber_conn) => zaber_conn,
        Err(e) => {
            return Err(anyhow!(
                "Failed to open Zaber serial port '{}': {}",
                config.serial_device,
                e
            ))
        }
    };

    zaber_conn.command_reply_n("system restore", 2, check::unchecked())?;

    let _ = zaber_conn.command_reply_n("home", 2, check::flag_ok());

    zaber_conn.poll_until_idle(1, check::flag_ok())?;
    zaber_conn.poll_until_idle(2, check::flag_ok())?;

    zaber_conn.command_reply_n("set comm.alert 0", 2, check::flag_ok())?;

    if config.offset_coax > 0. {
        zaber_conn
            .command_reply((1, format!("1 move rel {}", mm_to_steps(config.offset_coax))))?
            .flag_ok()?;
    } else if config.offset_coax < 0. {
        zaber_conn
            .command_reply((1, format!("1 move rel {}", mm_to_steps(config.offset_coax.abs()))))?
            .flag_ok()?;
    }
    zaber_conn.poll_until_idle(1, check::flag_ok())?;

    zaber_conn
        .command_reply((1, format!("set maxspeed {}", mm_per_sec_to_steps_per_sec(config.maxspeed_coax))))?
        .flag_ok()?;
    zaber_conn
        .command_reply((1, format!("set limit.max {}", mm_to_steps(config.limit_max_coax))))?
        .flag_ok()?;
    zaber_conn
        .command_reply((1, format!("set limit.min {}", mm_to_steps(config.limit_min_coax))))?
        .flag_ok()?;

    zaber_conn
        .command_reply((2, format!("set maxspeed {}", mm_per_sec_to_steps_per_sec(config.maxspeed_cross))))?
        .flag_ok()?;
    zaber_conn
        .command_reply((2, format!("set limit.max {}", mm_to_steps(config.limit_max_cross))))?
        .flag_ok()?;
    zaber_conn
        .command_reply((2, format!("set limit.min {}", mm_to_steps(config.limit_min_cross))))?
        .flag_ok()?;

    // 8.5 links
    zaber_conn
        .command_reply((1, "lockstep 1 setup enable 1 2"))?
        .flag_ok()?;

    let zaber_conn = Rc::new(RefCell::new(zaber_conn));

    let get_pos = || get_pos_zaber(Rc::clone(&zaber_conn));
    let move_coax = |pos| move_coax_zaber(Rc::clone(&zaber_conn), pos);
    let move_cross = |pos| move_cross_zaber(Rc::clone(&zaber_conn), pos);

    match config.backend {
        utils::Backend::Manual => {
            let voltage_shared = Arc::clone(&state.voltage_manual);
            let voltage = Rc::new(RefCell::new(0.));
            let get_voltage =
                move || get_voltage_manual(Rc::clone(&voltage), Arc::clone(&voltage_shared));
            return run(state, get_voltage, get_pos, move_coax, move_cross);
        }
        _ => {
            let get_voltage = || get_voltage_zaber(Rc::clone(&zaber_conn));
            return run(state, get_voltage, get_pos, move_coax, move_cross);
        }
    };
}

pub fn get_voltage_zaber<T: Backend>(zaber_conn: Rc<RefCell<ZaberConn<T>>>) -> Result<f64> {
    let cmd = format!("io get ai 1");
    let reply = zaber_conn.borrow_mut().command_reply((2, cmd))?.flag_ok()?;
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

pub fn get_pos_zaber<T: Backend>(
    zaber_conn: Rc<RefCell<ZaberConn<T>>>,
) -> Result<(f64, f64, bool, bool)> {
    let mut pos_coax = 0;
    let mut busy_coax = false;
    let mut pos_cross = 0;
    let mut busy_cross = false;
    for reply in zaber_conn.borrow_mut().command_reply_n_iter("get pos", 2)? {
        let reply = reply?.check(check::unchecked())?;
        match reply.target().device() {
            1 => {
                pos_coax = reply
                    .data()
                    .split_whitespace()
                    .next()
                    .ok_or(anyhow!("only one value returned"))?
                    .parse()?;
                busy_coax = if reply.status() == Status::Busy {
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
    return Ok((
        steps_to_mm(pos_coax),
        steps_to_mm(pos_cross),
        busy_coax,
        busy_cross,
    ));
}

pub fn move_coax_zaber<T: Backend>(
    zaber_conn: Rc<RefCell<ZaberConn<T>>>,
    pos: f64,
) -> Result<()> {
    let cmd = format!("lockstep 1 move abs {}", mm_to_steps(pos));
    let _ = zaber_conn.borrow_mut().command_reply((1, cmd))?.flag_ok()?;
    Ok(())
}

pub fn move_cross_zaber<T: Backend>(zaber_conn: Rc<RefCell<ZaberConn<T>>>, pos: f64) -> Result<()> {
    let cmd = format!("move abs {}", mm_to_steps(pos));
    let _ = zaber_conn.borrow_mut().command_reply((2, cmd))?.flag_ok()?;
    Ok(())
}

pub fn steps_to_mm(steps: u64) -> f64 {
    steps as f64 * MICROSTEP_SIZE / 1000.
}

pub fn mm_to_steps(millis: f64) -> u64 {
    (millis * 1000. / MICROSTEP_SIZE) as u64
}

pub fn mm_per_sec_to_steps_per_sec(millis_per_s: f64) -> u64 {
    (millis_per_s * 1000. * VELOCITY_FACTOR / MICROSTEP_SIZE) as u64
}

pub fn steps_per_sec_to_mm_per_sec(steps_per_sec: u64) -> f64 {
    steps_per_sec as f64 * MICROSTEP_SIZE / 1000. / VELOCITY_FACTOR
}
