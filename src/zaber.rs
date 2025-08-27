use crate::{simulation::Simulator, utils::Config};
use anyhow::{anyhow, Result};
use tokio::task;
use zproto::ascii::port::handlers::SendHandlers;
use zproto::ascii::port::{DefaultTag, OpenGeneralOptions};
use zproto::ascii::response::{check, Status};
use zproto::backend::Serial;

type Port<T> = zproto::ascii::Port<'static, T, DefaultTag, SendHandlers<'static>>;

pub const MICROSTEP_SIZE: f64 = 0.49609375; //µm
                                            // pub const VELOCITY_FACTOR: f64 = 1.6384;
pub const MAX_POS: u32 = 201574; // microsteps
pub const MAX_SPEED: u32 = 153600; // microsteps/sec

pub trait AxisBackend {
    fn get_pos(&mut self) -> Result<([bool; 2], [u32; 2])>;
    fn move_axis(&mut self, axis_index: usize, target_pos: u32) -> Result<()>;
}

pub async fn get_axis_port(config: &Config) -> Result<Box<dyn AxisBackend + Send>> {
    match config.mock_zaber {
        true => return Ok(Box::new(MockZaberPort::new(config).await?)),
        false => return Ok(Box::new(ZaberPort::new(config).await?)),
    }
}

pub struct ZaberPort {
    pub port: Port<Serial>,
}

impl ZaberPort {
    pub async fn new(config: &Config) -> Result<Self> {
        return match zproto::ascii::Port::open_serial(&config.serial_device) {
            Ok(port) => {
                let mut port = port.try_into_send()
                    .map_err(|_| anyhow!("Failed to convert zaber port into send"))?;
                init_axes(&mut port, &config).await?;
                return Ok(ZaberPort { port });
            }
            Err(e) => Err(anyhow!(
                "Failed to open Zaber serial port '{}': {}",
                config.serial_device,
                e
            )),
        };
    }
}

impl AxisBackend for ZaberPort {
    fn get_pos(&mut self) -> Result<([bool; 2], [u32; 2])> {
        get_pos_zaber(&mut self.port)
    }

    fn move_axis(&mut self, axis_index: usize, target_pos: u32) -> Result<()> {
        match axis_index {
            1 => move_coax_zaber(&mut self.port, target_pos),
            2 => move_cross_zaber(&mut self.port, target_pos),
            _ => Err(anyhow!("Invalid axis index {}", axis_index)),
        }
    }
}

pub struct MockZaberPort {
    pub port: Port<Simulator>,
}

impl MockZaberPort {
    pub async fn new(config: &Config) -> Result<Self> {
        let sim = Simulator::new();
        let mut opt = OpenGeneralOptions::new();
        opt.checksums(false);
        opt.message_ids(false);
        let sim = opt.open(sim);
        let mut sim = sim.try_into_send()
            .map_err(|_| anyhow!("Failed to convert zaber port into send"))?;
        init_axes(&mut sim, &config).await?;
        return Ok(MockZaberPort { port: sim });
    }
}

impl AxisBackend for MockZaberPort {
    fn get_pos(&mut self) -> Result<([bool; 2], [u32; 2])> {
        get_pos_zaber(&mut self.port)
    }

    fn move_axis(&mut self, axis_index: usize, target_pos: u32) -> Result<()> {
        match axis_index {
            1 => move_coax_zaber(&mut self.port, target_pos),
            2 => move_cross_zaber(&mut self.port, target_pos),
            _ => Err(anyhow!("Invalid axis index {}", axis_index)),
        }
    }
}

async fn init_axes<T>(zaber_conn: &mut Port<T>, config: &Config) -> Result<()>
where
    T: zproto::backend::Backend,
{
    zaber_conn.command_reply_n("system restore", 2, check::unchecked())
        .map_err(|_| anyhow!("Failed restore axes"))?;
    task::yield_now().await;

    let _ = zaber_conn.command_reply_n("home", 2, check::flag_ok());
    task::yield_now().await;

    zaber_conn.poll_until_idle(1, check::flag_ok())
        .map_err(|_| anyhow!("Failed to wait for coaxial axis to be idle"))?;
    task::yield_now().await;

    zaber_conn.poll_until_idle(2, check::flag_ok())
        .map_err(|_| anyhow!("Failed to wait for cross axis to be idle"))?;
    task::yield_now().await;

    zaber_conn.command_reply_n("set comm.alert 0", 2, check::flag_ok())?;
    task::yield_now().await;

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
    task::yield_now().await;
    zaber_conn.poll_until_idle(1, check::flag_ok())?;
    //.unwrap_or(Err(anyhow!("Failed to wait for offset axis to be idle"))?);
    task::yield_now().await;

    zaber_conn
        .command_reply((1, format!("set maxspeed {}", config.maxspeed_coax)))?
        .flag_ok()?;
    //.unwrap_or(Err(anyhow!("Failed to set max speed for coaxial axis"))?);
    task::yield_now().await;

    zaber_conn
        .command_reply((1, format!("set limit.max {}", config.limit_max_coax)))?
        .flag_ok()?;
    //.unwrap_or(Err(anyhow!("Failed to set max limit for coaxial axis"))?);
    task::yield_now().await;

    zaber_conn
        .command_reply((1, format!("set limit.min {}", config.limit_min_coax)))?
        .flag_ok()?;
    //.unwrap_or(Err(anyhow!("Failed to set min limit for coaxial axis"))?);
    task::yield_now().await;

    zaber_conn
        .command_reply((1, format!("set accel {}", config.accel_coax)))?
        .flag_ok()?;
    //.unwrap_or(Err(anyhow!("Failed to set acceleration for coaxial axis"))?);
    task::yield_now().await;

    zaber_conn
        .command_reply((2, format!("set maxspeed {}", config.maxspeed_cross)))?
        .flag_ok()?;
    //.unwrap_or(Err(anyhow!("Failed to set max speed for cross axis"))?);
    task::yield_now().await;

    zaber_conn
        .command_reply((2, format!("set limit.max {}", config.limit_max_cross)))?
        .flag_ok()?;
    //.unwrap_or(Err(anyhow!("Failed to set max limit for cross axis"))?);
    task::yield_now().await;

    zaber_conn
        .command_reply((2, format!("set limit.min {}", config.limit_min_cross)))?
        .flag_ok()?;
    //.unwrap_or(Err(anyhow!("Failed to set min limit for cross axis"))?);
    task::yield_now().await;

    zaber_conn
        .command_reply((2, format!("set accel {}", config.accel_cross)))?
        .flag_ok()?;
    //.unwrap_or(Err(anyhow!("Failed to set acceleration for cross axis"))?);
    task::yield_now().await;

    zaber_conn
        .command_reply((1, "lockstep 1 setup enable 1 2"))?
        .flag_ok()?;
    //.unwrap_or(Err(anyhow!("Failed to enable lockstep mode"))?);

    Ok(())
}

pub fn get_pos_zaber<T: zproto::backend::Backend>(
    zaber_conn: &mut Port<T>,
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
    zaber_conn: &mut Port<T>,
    pos: u32,
) -> Result<()> {
    let cmd = format!("lockstep 1 move abs {}", pos);
    let _ = zaber_conn.command_reply((1, cmd))?.flag_ok()?;
    Ok(())
}

pub fn move_cross_zaber<T: zproto::backend::Backend>(
    zaber_conn: &mut Port<T>,
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
