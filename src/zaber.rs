use crate::{simulation::Simulator, utils::Config};
use ads1x1x::ic::{Ads1115, Resolution16Bit};
use ads1x1x::mode::Continuous;
use ads1x1x::Ads1x1x;
use anyhow::{anyhow, Result};
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
pub type Adc = Ads1x1x<I2c<Ft232h>, Ads1115, Resolution16Bit, Continuous>;

pub fn init_zaber_mock(config: &Config) -> Result<ZaberConn<Simulator>> {
    let sim = Simulator::new();
    let mut opt = OpenGeneralOptions::new();
    opt.checksums(false);
    opt.message_ids(false);
    let mut sim = opt.open(sim);
    init_axes(&mut sim, &config)?;
    return Ok(sim);
}

pub fn init_zaber(
    config: &Config,
) -> Result<zproto::ascii::Port<'static, zproto::backend::Serial>, anyhow::Error> {
    return match Port::open_serial(&config.serial_device) {
        Ok(mut zaber_conn) => {
            init_axes(&mut zaber_conn, &config)?;
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
    //.unwrap_or(Err(anyhow!("Failed restore axes"))?);

    let _ = zaber_conn.command_reply_n("home", 2, check::flag_ok());

    zaber_conn.poll_until_idle(1, check::flag_ok())?;
    //.unwrap_or(Err(anyhow!("Failed to wait for coaxial axis to be idle"))?);
    zaber_conn.poll_until_idle(2, check::flag_ok())?;
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
    zaber_conn.poll_until_idle(1, check::flag_ok())?;
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
) -> Result<([bool; 2], [u32; 2])> {
    let mut pos = [0; 2];
    let mut is_busy = [false; 2];
    for reply in zaber_conn.command_reply_n_iter("get pos", 2)? {
        let reply = reply?.check(check::unchecked())?;
        match reply.target().device() {
            1 => {
                pos[0] = reply
                    .data()
                    .split_whitespace()
                    .next()
                    .ok_or(anyhow!("only one value returned"))?
                    .parse()?;
                is_busy[0] = if reply.status() == Status::Busy {
                    true
                } else {
                    false
                };
            }
            2 => {
                pos[1] = reply.data().parse()?;
                is_busy[1] = if reply.status() == Status::Busy {
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
    return Ok((is_busy, pos));
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
