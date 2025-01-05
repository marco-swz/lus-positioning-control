use std::io;
use std::sync::{Arc, RwLock};

use crate::{control::Backend, simulation::Simulator, utils::Config};
use ads1x1x::ic::{Ads1115, Resolution16Bit};
use ads1x1x::mode::OneShot;
use ads1x1x::{channel, Ads1x1x};
use anyhow::{anyhow, Result};
use chrono::Local;
use ftdi_embedded_hal::{libftd2xx::Ft232h, I2c};
use linux_embedded_hal::nb::block;
use zproto::ascii::command::MaxPacketSize;
use zproto::ascii::{
    response::{check, Status},
    Port,
};

pub const MICROSTEP_SIZE: f64 = 0.49609375; //µm
pub const VELOCITY_FACTOR: f64 = 1.6384;
pub const MAX_POS: u32 = 201574; // microsteps
pub const MAX_SPEED: f64 = 153600.; // microsteps/sec

pub type ZaberConn<T> = Port<'static, T>;
pub type Adc = Ads1x1x<I2c<Ft232h>, Ads1115, Resolution16Bit, OneShot>;

pub struct TrackingBackend<'a, T> {
    config: Config,
    zaber_conn: &'a mut ZaberConn<T>,
    adc: Adc,
    target_manual: (u32, u32, f64),
    target_manual_shared: Arc<RwLock<(u32, u32, f64)>>,
}

impl<'a, T> TrackingBackend<'a, T>
where
    T: zproto::backend::Backend,
{
    pub fn new(
        port: &'a mut ZaberConn<T>,
        config: Config,
        adc: Adc,
        target_shared: Arc<RwLock<(u32, u32, f64)>>,
    ) -> Result<Self> {
        init_axes(port, &config)?;
        Ok(TrackingBackend {
            config,
            zaber_conn: port,
            adc,
            target_manual: (0, 0, 0.),
            target_manual_shared: target_shared,
        })
    }
}

impl<T> Backend for TrackingBackend<'_, T>
where
    T: zproto::backend::Backend,
{
    fn get_target(&mut self) -> Result<(u32, u32, f64)> {
        let Ok(raw) = block!(self.adc.read(channel::DifferentialA0A1)) else {
            return Err(anyhow!("Failed to read from ADC"));
        };
        let voltage = raw as f64 * 4.069 / 32767.; // 65536.;
        tracing::debug!("voltage read {}", voltage);

        let target_coax = voltage_to_steps(
            voltage,
            (self.config.voltage_min, self.config.voltage_max),
            (self.config.limit_min_coax, self.config.limit_max_coax),
        );

        let target = match self.target_manual_shared.try_read() {
            Ok(shared) => (target_coax, shared.1),
            Err(_) => (target_coax, self.target_manual.1),
        };

        return Ok((target.0, target.1, voltage));
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

pub struct ManualBackend<'a, T> {
    zaber_conn: &'a mut ZaberConn<T>,
    target: (u32, u32, f64),
    target_shared: Arc<RwLock<(u32, u32, f64)>>,
}

impl<'a, T> ManualBackend<'a, T>
where
    T: zproto::backend::Backend,
{
    pub fn new(
        port: &'a mut ZaberConn<T>,
        config: Config,
        target_shared: Arc<RwLock<(u32, u32, f64)>>,
    ) -> Result<Self> {
        init_axes(port, &config)?;
        Ok(ManualBackend {
            zaber_conn: port,
            target: (0, 0, 0.),
            target_shared,
        })
    }
}

impl<T> Backend for ManualBackend<'_, T>
where
    T: zproto::backend::Backend,
{
    fn get_target(&mut self) -> Result<(u32, u32, f64)> {
        let Ok(shared) = self.target_shared.try_read() else {
            return Ok((self.target.0, self.target.1, 0.));
        };
        self.target = *shared;

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
    return Ok(
        Port::from_backend(
        sim,
        false,
        false,
        MaxPacketSize::default(),
    ));
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

pub fn mm_per_sec_to_steps_per_sec(millis_per_s: f64) -> u32 {
    (millis_per_s * 1000. * VELOCITY_FACTOR / MICROSTEP_SIZE) as u32
}

pub fn steps_per_sec_to_mm_per_sec(steps_per_sec: f64) -> f64 {
    steps_per_sec * MICROSTEP_SIZE / 1000. / VELOCITY_FACTOR
}

pub fn voltage_to_steps(voltage: f64, voltage_range: (f64, f64), pos_range: (u32, u32)) -> u32 {
    return (pos_range.1 as f64
        - (pos_range.1 as f64 - pos_range.0 as f64) / (voltage_range.1 - voltage_range.0)
            * (voltage - voltage_range.0)) as u32;
}
