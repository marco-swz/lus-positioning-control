use anyhow::Result;
use core::str;
use std::io;
use std::io::Write;

use chrono::{DateTime, Duration, Local};

use crate::zaber::{MAX_POS, MAX_SPEED};

#[derive(Debug)]
pub struct Simulator {
    pub pos: [[u32; 2]; 2],
    pub offset: Option<u32>,
    pub busy: [[bool; 2]; 2],
    pub vel: [[f64; 2]; 2],
    pub time: DateTime<Local>,
    pub target: [[u32; 2]; 2],
    pub limit: [[[u32; 2]; 2]; 2],
    pub ignored_read_timeout: Option<std::time::Duration>,
    pub buffer: io::Cursor<Vec<u8>>,
}

impl Simulator {
    pub fn new() -> Self {
        Simulator {
            pos: [[0; 2], [0; 2]],
            offset: None,
            busy: [[false; 2], [false; 2]],
            time: Local::now(),
            target: [[0; 2], [0; 2]],
            limit: [[[0, MAX_POS], [0, MAX_POS]], [[0, MAX_POS], [0, MAX_POS]]],
            vel: [[MAX_SPEED; 2], [MAX_SPEED; 2]],
            ignored_read_timeout: None,
            buffer: io::Cursor::new(Vec::new()),
        }
    }

    pub fn step(&mut self, time_step: Duration) {
        self.pos[0][0] = move_axis(self.pos[0][0], self.target[0][0], self.vel[0][0], time_step);
        self.pos[0][1] = move_axis(self.pos[0][1], self.target[0][1], self.vel[0][1], time_step);
        self.pos[1][0] = move_axis(self.pos[1][0], self.target[1][0], self.vel[1][0], time_step);
        self.time = self.time + time_step;
    }

    pub fn get_pos(&mut self) {
        assert!(self.offset.is_some());

        let mut msg = self.get_pos_axis(0, 0);
        msg += &self.get_pos_axis(1, 0);

        write!(
            self.buffer,
            "{}",
            msg
        )
        .unwrap();
    }

    fn get_pos_axis(&self, device: usize, axis: usize) -> String {
        let busy = match self.busy[device][axis] {
            true => "BUSY",
            false => "IDLE",
        };

        format!("@0{} {} OK {} -- {}\r\n", device, axis, busy, self.pos[device][axis])
    }

    fn move_abs_axis(&mut self, device: usize, axis: usize, target: u32) -> String {
        let busy = match self.busy[device][axis] {
            true => "BUSY",
            false => "IDLE",
        };

        if target < self.limit[device][axis][0] || target > self.limit[device][axis][1] {
            return format!("@0{} {} RJ BUSY WR BADDATA\r\n", device, axis);
        }

        self.target[device][axis] = target;

        format!("@0{} {} OK {} -- 0\r\n", device, axis, busy)
    }

    pub fn move_abs(&mut self, device: Option<usize>, axis: Option<usize>, target: u32) {
        assert!(self.offset.is_some());

        let msg = match device {
            Some(d) => match axis {
                Some(a) => self.move_abs_axis(d, a, target),
                None => {
                    let mut msg = self.move_abs_axis(d, 0, target);
                    msg += &self.move_abs_axis(d, 1, target);
                    msg
                }
            }
            None => {
                    let mut msg = self.move_abs_axis(0, 0, target);
                    msg += &self.move_abs_axis(0, 1, target);
                    msg += &self.move_abs_axis(1, 0, target);
                    msg
            }
        };

        write!(
            self.buffer,
            "{}",
            msg
        )
        .unwrap();
    }

    pub fn set_limit(&mut self, device: usize, axis: usize, limit: u32, is_max: bool) {
        if is_max {
            self.limit[device][axis][1] = limit;
        } else {
            self.limit[device][axis][0] = limit;
        }

        let busy = match self.busy[device][axis] {
            true => "BUSY",
            false => "IDLE",
        };

        write!(
            self.buffer,
            "{}",
            format!("@0{} {} OK {} -- 0\r\n", device, axis, busy)
        )
        .unwrap();
    }

    fn home_axis(&mut self, device: usize, axis: usize) -> String {
        self.target[device][axis] = 0;

        let status = match self.busy[device][axis] {
            true => "BUSY",
            false => "IDLE",
        };

        format!(
            "@0{} {} OK {} -- 0\r\n",
            device, axis, status, 
        )
    }

    pub fn home(&mut self) {
        let mut msg = self.home_axis(0, 0);
        msg += &self.home_axis(0, 1);
        msg += &self.home_axis(1, 0);

        write!(
            self.buffer,
            "{}",
            msg
        )
        .unwrap();
    }

    pub fn system_restore(&mut self) {
        *self = Self::new();
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
        self.buffer.get_mut().clear();

        self.step(Local::now().signed_duration_since(self.time));

        let (device, axis) = match str::from_utf8(&buf[1..3]) {
            Err(_) => (None, None),
            Ok(d) => match str::from_utf8(&buf[4..6]) {
                Err(_) => panic!("Invalid message"),
                Ok(a) => match a {
                    0 => (Some(d), None),
                    _ => (Some(d), Some(a)),
                }
            }
        };

        match buf {
            b"/get pos\n" => self.get_pos(),
            b"/system restore\n" => self.system_restore(),
            b"/home\n" => self.home(),
            b"/set comm.alert 0\n" => (),
            s if s.starts_with(b"/1 lockstep 1 move abs") => {
                self.move_abs(1, str::from_utf8(&s[23..]).unwrap().trim().parse().unwrap())
            }
            s if s.starts_with(b"/2 move abs") => {
                self.move_abs(2, str::from_utf8(&s[12..]).unwrap().trim().parse().unwrap())
            }
            s if s.starts_with(b"/1 set limit.max") => {
                self.set_limit_max(1, str::from_utf8(&s[17..]).unwrap().trim().parse().unwrap())
            }
            s if s.starts_with(b"/2 set limit.max") => {
                self.set_limit_max(2, str::from_utf8(&s[17..]).unwrap().trim().parse().unwrap())
            }
            s if s.starts_with(b"/1 set limit.min") => {
                self.set_limit_min(1, str::from_utf8(&s[17..]).unwrap().trim().parse().unwrap())
            }
            s if s.starts_with(b"/2 set limit.min") => {
                self.set_limit_min(2, str::from_utf8(&s[17..]).unwrap().trim().parse().unwrap())
            }
            _ => panic!("unexpected message: {:?}", buf),
        };

        self.buffer.set_position(0);
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
    use crate::zaber::ZaberConn;

    use super::{move_axis, Simulator};
    use chrono::Duration;
    use zproto::ascii::{command::MaxPacketSize, response::{check, Status}, Port};

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
        let mut sim = Simulator::new();
        sim.lockstep = true;
        sim.pos_cross = 100;
        sim.pos_coax1 = 2000;
        sim.busy_cross = true;

        let mut port: ZaberConn<Simulator> =
            Port::from_backend(sim, false, false, MaxPacketSize::default());
        let resp = port
            .command_reply((2, "move abs 3000"))
            .unwrap()
            .flag_ok()
            .unwrap();

        assert_eq!(resp.target().device(), 2);
        assert_eq!(resp.target().axis(), 0);
        assert_eq!(resp.status(), Status::Busy);
        assert_eq!(resp.data(), "100");

        let resp = port
            .command_reply((1, "move abs 3000"))
            .unwrap()
            .flag_ok()
            .unwrap();

        assert_eq!(resp.target().device(), 1);
        assert_eq!(resp.target().axis(), 0);
        assert_eq!(resp.status(), Status::Idle);
        assert_eq!(resp.data(), "2000");
    }

    #[test]
    fn test_sim_get_pos() {
        let mut sim = Simulator::new();
        sim.lockstep = true;
        sim.pos_cross = 100;
        sim.pos_coax1 = 2000;
        sim.busy_cross = true;

        let mut port: ZaberConn<Simulator> =
            Port::from_backend(sim, false, false, MaxPacketSize::default());
        let resp = port.command_reply_n("get pos", 2, check::flag_ok()).unwrap();

        assert_eq!(resp[0].target().device(), 1);
        assert_eq!(resp[0].target().axis(), 0);
        assert_eq!(resp[0].status(), Status::Idle);
        assert_eq!(resp[0].warning(), "--");
        assert_eq!(resp[0].data(), "2000");

        assert_eq!(resp[1].target().device(), 2);
        assert_eq!(resp[1].target().axis(), 0);
        assert_eq!(resp[1].status(), Status::Busy);
        assert_eq!(resp[1].warning(), "--");
        assert_eq!(resp[1].data(), "100");
    }
}
