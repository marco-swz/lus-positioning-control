use std::sync::{Arc, RwLock};

use crate::{control::Backend, simulation::Simulator, utils::Config};
use ads1x1x::ic::{Ads1115, Resolution16Bit};
use ads1x1x::mode::OneShot;
use ads1x1x::Ads1x1x;
use anyhow::{anyhow, Result};
use evalexpr::Value;
use ftdi_embedded_hal::{libftd2xx::Ft232h, I2c};
use zproto::ascii::port::OpenGeneralOptions;
use zproto::ascii::{
    response::{check, Status},
    Port,
};

pub const MICROSTEP_SIZE: f64 = 0.49609375; //µm
                                            // pub const VELOCITY_FACTOR: f64 = 1.6384;
pub const MAX_POS: u32 = 201574; // microsteps
pub const MAX_SPEED: u32 = 153600; // microsteps/sec

pub type ZaberConn<T> = Port<'static, T>;
pub type Adc = Ads1x1x<I2c<Ft232h>, Ads1115, Resolution16Bit, OneShot>;

pub struct TrackingBackend<'a, T, U> {
    zaber_conn: &'a mut ZaberConn<T>,
    fn_voltage: fn(&mut U) -> Result<[f64; 2]>,
    adc: U,
    formula_coax: evalexpr::Node,
    formula_cross: evalexpr::Node,
}

impl<'a, T, U> TrackingBackend<'a, T, U>
where
    T: zproto::backend::Backend,
{
    pub fn new(
        port: &'a mut ZaberConn<T>,
        config: Config,
        adc: U,
        fn_voltage: fn(&mut U) -> Result<[f64; 2]>,
    ) -> Result<Self> {
        init_axes(port, &config)?;
        let formula_cross = config.formula_cross.clone();
        let formula_coax = config.formula_coax.clone();
        Ok(TrackingBackend {
            zaber_conn: port,
            adc,
            fn_voltage,
            formula_cross: evalexpr::build_operator_tree(&formula_cross)?,
            formula_coax: evalexpr::build_operator_tree(&formula_coax)?,
        })
    }
}

impl<T, U> Backend for TrackingBackend<'_, T, U>
where
    T: zproto::backend::Backend,
{
    fn get_target(&mut self) -> Result<(u32, u32, f64, f64)> {
        let voltage = (self.fn_voltage)(&mut self.adc)?;
        let context = evalexpr::context_map! {
            "v1" => Value::Float(voltage[0]),
            "v2" => Value::Float(voltage[1]),
        }?;

        let target_coax_mm = self.formula_coax.eval_number_with_context(&context)?;
        let target_coax = mm_to_steps(target_coax_mm);
        let target_cross_mm = self.formula_cross.eval_number_with_context(&context)?;
        let target_cross = mm_to_steps(target_cross_mm);

        return Ok((target_coax, target_cross, voltage[0], voltage[1]));
    }

    fn get_pos(&mut self) -> Result<(u32, u32, bool, bool)> {
        return get_pos_zaber(&mut self.zaber_conn);
    }

    fn move_coax(&mut self, target: u32) -> Result<()> {
        return move_coax_zaber(&mut self.zaber_conn, target);
    }

    fn move_cross(&mut self, target: u32) -> Result<()> {
        return move_cross_zaber(&mut self.zaber_conn, target);
    }
}

pub struct ManualBackend<'a, T, U> {
    zaber_conn: &'a mut ZaberConn<T>,
    adc: U,
    fn_voltage: fn(&mut U) -> Result<[f64; 2]>,
    target: (u32, u32, f64, f64),
    target_shared: Arc<RwLock<(u32, u32, f64, f64)>>,
}

impl<'a, T, U> ManualBackend<'a, T, U>
where
    T: zproto::backend::Backend,
{
    pub fn new(
        port: &'a mut ZaberConn<T>,
        adc: U,
        config: Config,
        fn_voltage: fn(&mut U) -> Result<[f64; 2]>,
        target_shared: Arc<RwLock<(u32, u32, f64, f64)>>,
    ) -> Result<Self> {
        init_axes(port, &config)?;
        Ok(ManualBackend {
            zaber_conn: port,
            fn_voltage,
            adc,
            target: (0, 0, 0., 0.),
            target_shared,
        })
    }
}

impl<T, U> Backend for ManualBackend<'_, T, U>
where
    T: zproto::backend::Backend,
{
    fn get_target(&mut self) -> Result<(u32, u32, f64, f64)> {
        let voltage = (self.fn_voltage)(&mut self.adc)?;
        let Ok(shared) = self.target_shared.try_read() else {
            return Ok((self.target.0, self.target.1, voltage[0], voltage[1]));
        };
        let shared = (shared.0, shared.1, voltage[0], voltage[1]);
        self.target = shared;

        return Ok(self.target);
    }

    fn get_pos(&mut self) -> Result<(u32, u32, bool, bool)> {
        return get_pos_zaber(&mut self.zaber_conn);
    }

    fn move_coax(&mut self, target: u32) -> Result<()> {
        return move_coax_zaber(&mut self.zaber_conn, target);
    }

    fn move_cross(&mut self, target: u32) -> Result<()> {
        return move_cross_zaber(&mut self.zaber_conn, target);
    }
}

pub fn init_zaber_mock() -> Result<ZaberConn<Simulator>> {
    let sim = Simulator::new();
    let mut opt = OpenGeneralOptions::new();
    opt.checksums(false);
    opt.message_ids(false);
    return Ok(opt.open(sim));
}

pub fn init_zaber(
    config: Config,
) -> Result<zproto::ascii::Port<'static, zproto::backend::Serial>, anyhow::Error> {
    return match Port::open_serial(&config.serial_device) {
        Ok(zaber_conn) => {
            return Ok(zaber_conn);
        }
        Err(e) => Err(anyhow!(
            "Failed to open Zaber serial port '{}': {}",
            config.serial_device,
            e
        )),
    };
}

fn init_axes<T>(zaber_conn: &mut ZaberConn<T>, config: &Config) -> Result<()>
where
    T: zproto::backend::Backend,
{
    zaber_conn
        .command_reply_n("system restore", 2, check::unchecked())?;
        //.unwrap_or(Err(anyhow!("Failed restore axes"))?);

    let _ = zaber_conn.command_reply_n("home", 2, check::flag_ok());

    zaber_conn
        .poll_until_idle(1, check::flag_ok())?;
        //.unwrap_or(Err(anyhow!("Failed to wait for coaxial axis to be idle"))?);
    zaber_conn
        .poll_until_idle(2, check::flag_ok())?;
        //.unwrap_or(Err(anyhow!("Failed to wait for cross axis to be idle"))?);

    zaber_conn.command_reply_n("set comm.alert 0", 2, check::flag_ok())?;

    if config.offset_coax > 0 {
        zaber_conn
            .command_reply((1, format!("1 move rel {}", config.offset_coax)))?
            .flag_ok()?;
            //.unwrap_or(Err(anyhow!("Failed to set up coax offset"))?);
    } else if config.offset_coax < 0 {
        zaber_conn
            .command_reply((1, format!("2 move rel {}", config.offset_coax.abs())))?
            .flag_ok()?;
            //.unwrap_or(Err(anyhow!("Failed to set up coax offset"))?);
    }
    zaber_conn
        .poll_until_idle(1, check::flag_ok())?;
        //.unwrap_or(Err(anyhow!("Failed to wait for offset axis to be idle"))?);

    zaber_conn
        .command_reply((1, format!("set maxspeed {}", config.maxspeed_coax)))?
        .flag_ok()?;
        //.unwrap_or(Err(anyhow!("Failed to set max speed for coaxial axis"))?);
    zaber_conn
        .command_reply((1, format!("set limit.max {}", config.limit_max_coax)))?
        .flag_ok()?;
        //.unwrap_or(Err(anyhow!("Failed to set max limit for coaxial axis"))?);
    zaber_conn
        .command_reply((1, format!("set limit.min {}", config.limit_min_coax)))?
        .flag_ok()?;
        //.unwrap_or(Err(anyhow!("Failed to set min limit for coaxial axis"))?);
    zaber_conn
        .command_reply((1, format!("set accel {}", config.accel_coax)))?
        .flag_ok()?;
        //.unwrap_or(Err(anyhow!("Failed to set acceleration for coaxial axis"))?);

    zaber_conn
        .command_reply((2, format!("set maxspeed {}", config.maxspeed_cross)))?
        .flag_ok()?;
        //.unwrap_or(Err(anyhow!("Failed to set max speed for cross axis"))?);
    zaber_conn
        .command_reply((2, format!("set limit.max {}", config.limit_max_cross)))?
        .flag_ok()?;
        //.unwrap_or(Err(anyhow!("Failed to set max limit for cross axis"))?);
    zaber_conn
        .command_reply((2, format!("set limit.min {}", config.limit_min_cross)))?
        .flag_ok()?;
        //.unwrap_or(Err(anyhow!("Failed to set min limit for cross axis"))?);
    zaber_conn
        .command_reply((2, format!("set accel {}", config.accel_cross)))?
        .flag_ok()?;
        //.unwrap_or(Err(anyhow!("Failed to set acceleration for cross axis"))?);

    zaber_conn
        .command_reply((1, "lockstep 1 setup enable 1 2"))?
        .flag_ok()?;
        //.unwrap_or(Err(anyhow!("Failed to enable lockstep mode"))?);

    Ok(())
}

pub fn get_pos_zaber<T: zproto::backend::Backend>(
    zaber_conn: &mut ZaberConn<T>,
) -> Result<(u32, u32, bool, bool)> {
    let mut pos_coax = 0;
    let mut busy_coax = false;
    let mut pos_cross = 0;
    let mut busy_cross = false;
    for reply in zaber_conn.command_reply_n_iter("get pos", 2)? {
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

pub fn move_coax_zaber<T: zproto::backend::Backend>(
    zaber_conn: &mut ZaberConn<T>,
    pos: u32,
) -> Result<()> {
    let cmd = format!("lockstep 1 move abs {}", pos);
    let _ = zaber_conn.command_reply((1, cmd))?.flag_ok()?;
    Ok(())
}

pub fn move_cross_zaber<T: zproto::backend::Backend>(
    zaber_conn: &mut ZaberConn<T>,
    pos: u32,
) -> Result<()> {
    let cmd = format!("move abs {}", pos);
    let _ = zaber_conn.command_reply((2, cmd))?.flag_ok()?;
    Ok(())
}

pub fn steps_to_mm(steps: u32) -> f64 {
    steps as f64 * MICROSTEP_SIZE / 1000.
}

pub fn mm_to_steps(millis: f64) -> u32 {
    (millis * 1000. / MICROSTEP_SIZE) as u32
}
