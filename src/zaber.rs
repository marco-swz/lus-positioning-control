use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, RwLock},
};

use crate::{
    control::run,
    utils::{self, Config, ControlStatus, ExecState},
};
use ads1x1x::ic::{Ads1115, Resolution16Bit};
use ads1x1x::mode::OneShot;
use ads1x1x::{channel, Ads1x1x, FullScaleRange, TargetAddr};
use anyhow::{anyhow, Result};
//use ch341::i2c::{self, I2cDevice};
use linux_embedded_hal::nb::block;
use zproto::{
    ascii::{
        response::{check, Status},
        Port,
    },
    backend::Backend,
};

pub const MICROSTEP_SIZE: f64 = 0.49609375; //µm
pub const VELOCITY_FACTOR: f64 = 1.6384;
pub const MAX_POS: u32 = 201574; // microsteps
pub const MAX_SPEED: u32 = 153600; // microsteps/sec

type ZaberConn<T> = Port<'static, T>;

fn set_config<T: Backend>(mut zaber_conn: ZaberConn<T>, config: Config) -> Result<ZaberConn<T>> {
    zaber_conn.command_reply_n("system restore", 2, check::unchecked())?;

    let _ = zaber_conn.command_reply_n("home", 2, check::flag_ok());

    zaber_conn.poll_until_idle(1, check::flag_ok())?;
    zaber_conn.poll_until_idle(2, check::flag_ok())?;

    zaber_conn.command_reply_n("set comm.alert 0", 2, check::flag_ok())?;

    if config.offset_coax > 0 {
        zaber_conn
            .command_reply((1, format!("1 move rel {}", config.offset_coax)))?
            .flag_ok()?;
    } else if config.offset_coax < 0 {
        zaber_conn
            .command_reply((1, format!("1 move rel {}", config.offset_coax.abs())))?
            .flag_ok()?;
    }
    zaber_conn.poll_until_idle(1, check::flag_ok())?;

    zaber_conn
        .command_reply((
            1,
            format!(
                "set maxspeed {}",
                mm_per_sec_to_steps_per_sec(config.maxspeed_coax)
            ),
        ))?
        .flag_ok()?;
    zaber_conn
        .command_reply((1, format!("set limit.max {}", config.limit_max_coax)))?
        .flag_ok()?;
    zaber_conn
        .command_reply((1, format!("set limit.min {}", config.limit_min_coax)))?
        .flag_ok()?;

    zaber_conn
        .command_reply((
            2,
            format!(
                "set maxspeed {}",
                mm_per_sec_to_steps_per_sec(config.maxspeed_cross)
            ),
        ))?
        .flag_ok()?;
    zaber_conn
        .command_reply((2, format!("set limit.max {}", config.limit_max_cross)))?
        .flag_ok()?;
    zaber_conn
        .command_reply((2, format!("set limit.min {}", config.limit_min_cross)))?
        .flag_ok()?;

    zaber_conn
        .command_reply((1, "lockstep 1 setup enable 1 2"))?
        .flag_ok()?;

    Ok(zaber_conn)
}

pub fn init_zaber(state: &mut ExecState) -> Result<()> {
    let config = {
        let s = state.config.read().unwrap();
        s.clone()
    };

    if config.mock_zaber {
            let mut port = Port::open_mock();
            port.backend_mut()
                .set_write_callback(|message, reply_buffer| {
                    match message {
                        b"/1 io get ai 1\n" => write!(reply_buffer, "@01 0 OK BUSY -- 5.5\r\n"),
                        b"/get pos\n" => write!(
                            reply_buffer,
                            "@01 0 OK BUSY -- 20\r\n@02 0 OK BUSY -- 10.1\r\n"
                        ),
                        _ => panic!("unexpected message"),
                    }
                    .unwrap()
                });
            return port;
    }

    return match Port::open_serial(&serial_device) {
        Ok(zaber_conn) => zaber_conn,
        Err(e) => {
            return Err(anyhow!(
                "Failed to open Zaber serial port '{}': {}",
                serial_device,
                e
            ))
        }
    };



    let zaber_conn = Rc::new(RefCell::new(zaber_conn));

    let get_pos = || get_pos_zaber(Rc::clone(&zaber_conn));
    let move_coax = |pos| move_coax_zaber(Rc::clone(&zaber_conn), pos);
    let move_cross = |pos| move_cross_zaber(Rc::clone(&zaber_conn), pos);

    let config_old = config.clone();
    loop {
        let result = match config.backend {
            utils::Backend::Manual => {
                let target_shared = Arc::clone(&state.target_manual);
                let target = Rc::new(RefCell::new((0, 0)));
                let get_target =
                    move || get_target_manual(Rc::clone(&target), Arc::clone(&target_shared));
                run(state, get_target, get_pos, move_coax, move_cross)
            }

            utils::Backend::Tracking => {
                /*
                            let Ok(dev) = i2c::new() else {
                                return Err(anyhow!("Failed to create I2C device"));
                            };
                            let mut adc = Ads1x1x::new_ads1115(dev, TargetAddr::default());
                            let Ok(_) = adc.set_full_scale_range(FullScaleRange::Within4_096V) else {
                                return Err(anyhow!("Failed set ADC range"));
                            };
                            let adc = Rc::new(RefCell::new(adc));
                            let get_target = move || {
                                get_target_tracking(
                                    Rc::clone(&adc),
                                    (config.voltage_min, config.voltage_max),
                                    (config.limit_min_coax, config.limit_max_coax),
                                )
                            };
                            return run(state, get_target, get_pos, move_coax, move_cross);
                */
                Ok(())
            }

            utils::Backend::Zaber => {
                let get_voltage = || {
                    get_target_zaber(
                        Rc::clone(&zaber_conn),
                        (config.voltage_min, config.voltage_max),
                        (config.limit_min_coax, config.limit_max_coax),
                    )
                };
                run(state, get_voltage, get_pos, move_coax, move_cross)
            }
        };

        // If only the backend changes,
        // zaber does not need to re-initalized.
        let config = state.config.read().unwrap();
        if config_old.backend == config.backend {
            return result;
        };
    }
}

pub fn get_target_zaber<T: Backend>(
    zaber_conn: Rc<RefCell<ZaberConn<T>>>,
    voltage_range: (f64, f64),
    pos_range: (u32, u32),
) -> Result<(u32, u32)> {
    let cmd = format!("io get ai 1");
    let reply = zaber_conn.borrow_mut().command_reply((2, cmd))?.flag_ok()?;
    let target_coax = voltage_to_steps(reply.data().parse()?, voltage_range, pos_range);
    // TODO(marco): Set target for cross axis
    let target_cross = 0;
    return Ok((target_coax, target_cross));
}

fn get_target_manual(
    target: Rc<RefCell<(u32, u32)>>,
    target_shared: Arc<RwLock<(u32, u32)>>,
) -> Result<(u32, u32)> {
    let ref mut target = *target.borrow_mut();
    let Ok(shared) = target_shared.try_read() else {
        return Ok(*target);
    };
    *target = *shared;

    return Ok(*shared);
}

//fn get_target_tracking(
//    adc: Rc<RefCell<Ads1x1x<I2cDevice, Ads1115, Resolution16Bit, OneShot>>>,
//    voltage_range: (f64, f64),
//    pos_range: (f64, f64),
//) -> Result<(f64, f64)> {
//    let ref mut adc = *adc.borrow_mut();
//    let Ok(raw) = block!(adc.read(channel::DifferentialA0A1)) else {
//        return Err(anyhow!("Failed to read from ADC"));
//    };
//    let voltage = raw as f64 * 4.069 / 32767.; // 65536.;
//    tracing::debug!("voltage read {}", voltage);
//
//    let target_coax = voltage_to_mm(voltage, voltage_range, pos_range);
//    // TODO(marco): Set target for cross axis
//    let target_cross = 0.;
//    return Ok((target_coax, target_cross));
//}

pub fn get_pos_zaber<T: Backend>(
    zaber_conn: Rc<RefCell<ZaberConn<T>>>,
) -> Result<(u32, u32, bool, bool)> {
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
    return Ok((pos_coax, pos_cross, busy_coax, busy_cross));
}

pub fn move_coax_zaber<T: Backend>(zaber_conn: Rc<RefCell<ZaberConn<T>>>, pos: u32) -> Result<()> {
    let cmd = format!("lockstep 1 move abs {}", pos);
    let _ = zaber_conn.borrow_mut().command_reply((1, cmd))?.flag_ok()?;
    Ok(())
}

pub fn move_cross_zaber<T: Backend>(zaber_conn: Rc<RefCell<ZaberConn<T>>>, pos: u32) -> Result<()> {
    let cmd = format!("move abs {}", pos);
    let _ = zaber_conn.borrow_mut().command_reply((2, cmd))?.flag_ok()?;
    Ok(())
}

pub fn steps_to_mm(steps: u32) -> f64 {
    steps as f64 * MICROSTEP_SIZE / 1000.
}

pub fn mm_to_steps(millis: f64) -> u32 {
    (millis * 1000. / MICROSTEP_SIZE) as u32
}

pub fn mm_per_sec_to_steps_per_sec(millis_per_s: f64) -> u32 {
    (millis_per_s * 1000. * VELOCITY_FACTOR / MICROSTEP_SIZE) as u32
}

pub fn steps_per_sec_to_mm_per_sec(steps_per_sec: u32) -> f64 {
    steps_per_sec as f64 * MICROSTEP_SIZE / 1000. / VELOCITY_FACTOR
}

pub fn voltage_to_steps(voltage: f64, voltage_range: (f64, f64), pos_range: (u32, u32)) -> u32 {
    return (pos_range.1 as f64
        - (pos_range.1 as f64 - pos_range.0 as f64) / (voltage_range.1 - voltage_range.0)
            * (voltage - voltage_range.0)) as u32;
}
