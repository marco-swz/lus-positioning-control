use anyhow::Result;
use core::str;
use std::io;
use std::io::Write;

use chrono::{DateTime, Duration, Local};

#[derive(Debug)]
pub struct Simulator {
    pub pos_coax1: u32,
    pub pos_coax2: u32,
    pub pos_cross: u32,
    pub busy_coax1: bool,
    pub busy_coax2: bool,
    pub busy_cross: bool,
    pub lockstep: bool,
    pub vel_cross: f64,
    pub vel_coax: f64,
    pub time: DateTime<Local>,
    pub target_coax1: u32,
    pub target_coax2: u32,
    pub target_cross: u32,
    pub ignored_read_timeout: Option<std::time::Duration>,
    pub buffer: io::Cursor<Vec<u8>>,
}

impl Simulator {
    pub fn step(&mut self, time_step: Duration) {
        self.pos_coax1 = move_axis(self.pos_coax1, self.target_coax1, self.vel_coax, time_step);
        self.pos_coax2 = move_axis(self.pos_coax2, self.target_coax2, self.vel_coax, time_step);
        self.pos_cross = move_axis(self.pos_cross, self.target_cross, self.vel_cross, time_step);
        self.time = self.time + time_step;
    }

    pub fn get_pos(&mut self) {
        assert!(self.lockstep);

        let busy_coax = match self.busy_coax1 {
            true => "BUSY",
            false => "IDLE",
        };
        let busy_cross = match self.busy_cross {
            true => "BUSY",
            false => "IDLE",
        };

        write!(
            self.buffer,
            "{}",
            format!(
                "@01 0 OK {} -- {}\r\n@02 0 OK {} -- {}\r\n",
                self.pos_coax1, busy_coax, self.pos_cross, busy_cross,
            )
        )
        .unwrap();
    }

    pub fn move_abs(&mut self, axis: u8, target: u32) {
        assert!(self.lockstep);

        let mut pos = self.pos_coax1;
        if axis == 1 {
            self.target_coax1 = target;
        } else if axis == 2 {
            self.target_cross = target;
            pos = self.pos_cross;
        } else {
            panic!("error move abs: invalid axis {axis}")
        }

        write!(
            self.buffer,
            "{}",
            format!("@0{} 0 OK BUSY -- {}\r\n", axis, pos,)
        )
        .unwrap();
        dbg!(str::from_utf8(self.buffer.get_ref()).unwrap());
    }

    pub fn is_empty(&self) -> bool {
		self.buffer.position() >= self.buffer.get_ref().len() as u64
	}
}

impl zproto::backend::Backend for Simulator {
    fn set_read_timeout(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> Result<(), std::io::Error> {
        self.ignored_read_timeout = timeout;
        Ok(())
    }

    fn read_timeout(&self) -> Result<Option<std::time::Duration>, io::Error> {
        Ok(self.ignored_read_timeout)
    }

    fn name(&self) -> Option<String> {
        Some(format!(
            "<simulator 0x{:x}>",
            std::ptr::from_ref(self) as usize
        ))
    }
}

impl io::Read for Simulator {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        dbg!(str::from_utf8(self.buffer.get_ref()).unwrap());
        if self.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Simulated timeout error",
            ));
        }
        self.buffer.read(buf)
    }
}

impl io::Write for Simulator {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match buf {
            b"/get pos\n" => self.get_pos(),
            s if s.starts_with(b"/1 lockstep 1 move abs") => self.move_abs(
                1,
                str::from_utf8(&s[23..]).unwrap().trim().parse().unwrap(),
            ),
            s if s.starts_with(b"/2 move abs") => self.move_abs(
                2,
                str::from_utf8(&s[12..]).unwrap().trim().parse().unwrap(),
            ),
            _ => panic!("unexpected message: {:?}", buf),
        };

        return Ok(buf.len());
    }

    fn flush(&mut self) -> io::Result<()> {
        return Ok(());
    }
}

fn move_axis(pos: u32, target: u32, mut vel: f64, time_step: Duration) -> u32 {
    if pos == target {
        return target;
    }

    if pos > target {
        vel = -vel;
    }

    let mut pos_new = (pos as f64
        + vel * time_step.num_seconds() as f64
        + vel * time_step.subsec_nanos() as f64 / 1.0e9) as u32;

    dbg!(pos_new);
    if pos < target && pos_new > target {
        pos_new = target
    }
    if pos > target && pos_new < target {
        pos_new = target
    }

    return pos_new;
}

#[cfg(test)]
mod tests {
    use std::io;

    use crate::zaber::ZaberConn;

    use super::{move_axis, Simulator};
    use chrono::{Duration, Local};
    use zproto::ascii::{command::MaxPacketSize, Port};

    #[test]
    fn test_move() {
        assert_eq!(
            31,
            move_axis(20, 100, 2., Duration::new(5, 5e8 as u32).unwrap())
        );
        assert_eq!(
            15,
            move_axis(20, 0, 2., Duration::new(2, 5e7 as u32).unwrap())
        );
        assert_eq!(
            25,
            move_axis(20, 25, 2., Duration::new(5, 5e8 as u32).unwrap())
        );
        assert_eq!(
            18,
            move_axis(20, 18, 2., Duration::new(2, 5e7 as u32).unwrap())
        );
        assert_eq!(
            20,
            move_axis(20, 20, 2., Duration::new(2, 5e7 as u32).unwrap())
        );
    }

    #[test]
    fn test_sim_move_abs() {
        let sim = Simulator {
            lockstep: true,
            pos_cross: 100,
            pos_coax1: 2000,
            pos_coax2: 0,
            busy_cross: true,
            busy_coax1: false,
            busy_coax2: false,
            time: Local::now(),
            target_cross: 0,
            target_coax1: 0,
            target_coax2: 0,
            vel_cross: 23000.,
            vel_coax: 23000.,
            ignored_read_timeout: None,
            buffer: io::Cursor::new(Vec::new()),
        };
        let mut port: ZaberConn<Simulator> =
            Port::from_backend(sim, false, false, MaxPacketSize::default());
        let _ = port
            .command_reply((2, "move abs 3000"))
            .unwrap()
            .flag_ok()
            .unwrap();
    }
}
