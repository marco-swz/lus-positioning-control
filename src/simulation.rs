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
    pub vel: [[u32; 2]; 2],
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

        for d in 0..2 {
            for a in 0..2 {
                self.busy[d][a] = self.target[d][a] != self.pos[d][a];
            }
        }
        self.time = self.time + time_step;
    }

    pub fn get_pos(&mut self) {
        assert!(self.offset.is_some());

        let mut msg = self.get_pos_axis(0, 0);
        msg += &self.get_pos_axis(1, 0);

        write!(self.buffer, "{}", msg).unwrap();
    }

    fn get_pos_axis(&self, device: usize, axis: usize) -> String {
        let busy = match self.busy[device][axis] {
            true => "BUSY",
            false => "IDLE",
        };

        format!(
            "@0{} 0 OK {} -- {}\r\n",
            device + 1,
            busy,
            self.pos[device][axis]
        )
    }

    fn move_abs_axis(&mut self, device: usize, axis: usize, target: u32) -> bool {
        if target < self.limit[device][axis][0] || target > self.limit[device][axis][1] {
            return false;
        }

        self.target[device][axis] = target;
        return true;
    }

    pub fn move_abs(&mut self, device: Option<usize>, axis: Option<usize>, target: u32) {
        assert!(self.offset.is_some());

        let msg: String = match device {
            Some(d) => match axis {
                Some(a) => {
                    let mut msg = format!("@0{} {} RJ BUSY WR BADDATA\r\n", d + 1, a + 1);
                    if self.move_abs_axis(d, a, target) {
                        msg = format!("@0{} {} OK BUSY -- 0\r\n", d + 1, a + 1);
                    }
                    msg
                }
                None => {
                    let mut msg = format!("@0{} 0 RJ BUSY WR BADDATA\r\n", d + 1);
                    if self.move_abs_axis(d, 0, target) && self.move_abs_axis(d, 1, target) {
                        msg = format!("@0{} 0 OK BUSY -- 0\r\n", d + 1);
                    }
                    msg
                }
            },
            None => {
                let mut msg_dev1 = format!("@01 0 RJ BUSY WR BADDATA\r\n");
                if self.move_abs_axis(0, 0, target) && self.move_abs_axis(0, 1, target) {
                    msg_dev1 = format!("@01 0 OK BUSY -- 0\r\n");
                }

                let mut msg_dev2 = format!("@02 0 RJ BUSY WR BADDATA\r\n");
                if self.move_abs_axis(1, 0, target) {
                    msg_dev2 = format!("@02 0 OK BUSY -- 0\r\n");
                }
                msg_dev1 + &msg_dev2
            }
        };

        write!(self.buffer, "{}", msg).unwrap();
    }

    pub fn set_limit(
        &mut self,
        device: Option<usize>,
        axis: Option<usize>,
        limit: u32,
        is_max: bool,
    ) {
        let mut idx = 0;
        if is_max {
            idx = 1;
        }

        let msg: String = match device {
            Some(d) => match axis {
                Some(a) => {
                    let mut msg = format!("@0{} {} RJ BUSY WR BADDATA\r\n", d + 1, a + 1);
                    if limit <= MAX_POS {
                        self.limit[d][a][idx] = limit;
                        msg = format!("@0{} {} OK BUSY -- 0\r\n", d + 1, a + 1);
                    }
                    msg
                }
                None => {
                    let mut msg = format!("@0{} 0 RJ BUSY WR BADDATA\r\n", d + 1);
                    if limit <= MAX_POS {
                        self.limit[d][0][idx] = limit;
                        self.limit[d][1][idx] = limit;
                        msg = format!("@0{} 0 OK BUSY -- 0\r\n", d + 1);
                    }
                    msg
                }
            },
            None => {
                let mut msg_dev1 = format!("@01 0 RJ BUSY WR BADDATA\r\n");
                if limit <= MAX_POS {
                    self.limit[0][0][idx] = limit;
                    self.limit[0][1][idx] = limit;
                    msg_dev1 = format!("@01 0 OK BUSY -- 0\r\n");
                }

                let mut msg_dev2 = format!("@02 0 RJ BUSY WR BADDATA\r\n");
                if limit <= MAX_POS {
                    self.limit[1][0][idx] = limit;
                    self.limit[1][1][idx] = limit;
                    msg_dev2 = format!("@02 0 OK BUSY -- 0\r\n");
                }
                msg_dev1 + &msg_dev2
            }
        };

        write!(self.buffer, "{}", msg,).unwrap();
    }

    fn home_axis(&mut self, device: usize, axis: usize) -> String {
        self.target[device][axis] = 0;

        let status = match self.busy[device][axis] {
            true => "BUSY",
            false => "IDLE",
        };

        format!("@0{} {} OK {} -- 0\r\n", device + 1, axis + 1, status,)
    }

    pub fn home(&mut self) {
        let mut msg = self.home_axis(0, 0);
        msg += &self.home_axis(0, 1);
        msg += &self.home_axis(1, 0);

        write!(self.buffer, "{}", msg).unwrap();
    }

    pub fn system_restore(&mut self) {
        *self = Self::new();
        write!(self.buffer, "@01 0 OK BUSY -- 0\r\n@02 0 OK BUSY -- 0\r\n").unwrap();
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.position() >= self.buffer.get_ref().len() as u64
    }

    pub fn move_rel(&mut self, device: Option<usize>, axis: Option<usize>, target: i32) {
        assert!(self.offset.is_some());

        let msg: String = match device {
            Some(d) => match axis {
                Some(a) => {
                    let mut msg = format!("@0{} {} RJ BUSY WR BADDATA\r\n", d + 1, a + 1);
                    if self.move_abs_axis(d, a, (self.target[d][a] as i32 + target) as u32) {
                        msg = format!("@0{} {} OK BUSY -- 0\r\n", d + 1, a + 1);
                    }
                    msg
                }
                None => {
                    let mut msg = format!("@0{} 0 RJ BUSY WR BADDATA\r\n", d + 1);
                    if self.move_abs_axis(d, 0, (self.target[d][0] as i32 + target) as u32)
                        && self.move_abs_axis(d, 1, (self.target[d][1] as i32 + target) as u32)
                    {
                        msg = format!("@0{} 0 OK BUSY -- 0\r\n", d + 1);
                    }
                    msg
                }
            },
            None => {
                let mut msg_dev1 = format!("@01 0 RJ BUSY WR BADDATA\r\n");
                if self.move_abs_axis(0, 0, (self.target[0][0] as i32 + target) as u32)
                    && self.move_abs_axis(0, 1, (self.target[0][1] as i32 + target) as u32)
                {
                    msg_dev1 = format!("@01 0 OK BUSY -- 0\r\n");
                }

                let mut msg_dev2 = format!("@02 0 RJ BUSY WR BADDATA\r\n");
                if self.move_abs_axis(1, 0, (self.target[1][0] as i32 + target) as u32) {
                    msg_dev2 = format!("@02 0 OK BUSY -- 0\r\n");
                }
                msg_dev1 + &msg_dev2
            }
        };

        write!(self.buffer, "{}", msg).unwrap();
    }

    fn lockstep_enable(&mut self) {
        self.offset = Some(self.pos[0][1] - self.pos[0][0]);
        write!(self.buffer, "@01 0 OK BUSY -- 0\r\n").unwrap();
    }

    fn poll(&mut self, device: Option<usize>) {
        let device = device.unwrap();
        let busy = match self.busy[device][0] {
            true => "BUSY",
            false => "IDLE",
        };

        let msg = format!("@0{} 0 OK {} -- 0\r\n", device + 1, busy,);
        write!(self.buffer, "{}", msg).unwrap();
    }

    fn set_maxspeed(&mut self, device: Option<usize>, vel: u32) {
        let device = device.unwrap();
        self.vel[device][0] = vel;
        self.vel[device][1] = vel;
        let msg = format!("@0{} 0 OK BUSY -- 0\r\n", device + 1);
        write!(self.buffer, "{}", msg).unwrap();
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

        let (device, axis, command_slice) =
            match str::from_utf8(&buf[1..2]).unwrap().parse::<usize>() {
                Err(_) => (None, None, 1..),
                Ok(d) => match str::from_utf8(&buf.get(3..4).unwrap_or(b"0"))
                    .unwrap()
                    .parse::<usize>()
                {
                    Err(_) => (Some(d - 1), None, 3..),
                    Ok(0) => (Some(d - 1), None, 6..),
                    Ok(a) => (Some(d - 1), Some(a - 1), 6..),
                },
            };

        match buf.get(command_slice).unwrap_or(b"") {
            b"" => self.poll(device),
            b"get pos\n" => self.get_pos(),
            b"home\n" => self.home(),
            b"set comm.alert 0\n" => {
                write!(self.buffer, "@01 0 OK BUSY -- 0\r\n@02 0 OK BUSY -- 0\r\n").unwrap()
            }
            b"lockstep 1 setup enable 1 2\n" => self.lockstep_enable(),
            s if s.starts_with(b"set accel ") => write!(
                self.buffer,
                "{}",
                format!("@0{} 0 OK BUSY -- 0\r\n", device.unwrap() + 1)
            )
            .unwrap(),
            s if s.starts_with(b"lockstep 1 move abs") => self.move_abs(
                device,
                axis,
                str::from_utf8(&s[20..]).unwrap().trim().parse().unwrap(),
            ),
            s if s.starts_with(b"system restore") => self.system_restore(),
            s if s.starts_with(b"move abs") => self.move_abs(
                device,
                axis,
                str::from_utf8(&s[9..]).unwrap().trim().parse().unwrap(),
            ),
            s if s.starts_with(b"move rel") => {
                self.move_rel(
                    device,
                    axis,
                    str::from_utf8(&s[9..]).unwrap().trim().parse().unwrap(),
                );
            }
            s if s.starts_with(b"set limit.max") => self.set_limit(
                device,
                axis,
                str::from_utf8(&s[14..]).unwrap().trim().parse().unwrap(),
                true,
            ),
            s if s.starts_with(b"set maxspeed") => self.set_maxspeed(
                device,
                str::from_utf8(&s[13..]).unwrap().trim().parse().unwrap(),
            ),
            s if s.starts_with(b"set limit.max") => self.set_limit(
                device,
                axis,
                str::from_utf8(&s[14..]).unwrap().trim().parse().unwrap(),
                true,
            ),
            s if s.starts_with(b"set limit.min") => self.set_limit(
                device,
                axis,
                str::from_utf8(&s[14..]).unwrap().trim().parse().unwrap(),
                false,
            ),
            s if s.starts_with(b"set limit.min") => self.set_limit(
                device,
                axis,
                str::from_utf8(&s[14..]).unwrap().trim().parse().unwrap(),
                false,
            ),
            _ => panic!("unexpected message: {:?}", str::from_utf8(buf).unwrap()),
        };

        self.buffer.set_position(0);
        return Ok(buf.len());
    }

    fn flush(&mut self) -> io::Result<()> {
        return Ok(());
    }
}

fn move_axis(pos: u32, target: u32, vel: u32, time_step: Duration) -> u32 {
    if pos == target {
        return target;
    }

    let mut vel: i64 = vel as i64;

    if pos > target {
        vel = -vel;
    }

    let mut pos_new = (pos as f64
        + vel as f64 * time_step.num_seconds() as f64
        + vel as f64 * time_step.subsec_nanos() as f64 / 1.0e9) as u32;

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
    use zproto::ascii::{
        command::MaxPacketSize,
        response::{check, Status},
        Port,
    };

    #[test]
    fn test_move() {
        assert_eq!(
            31,
            move_axis(20, 100, 2, Duration::new(5, 5e8 as u32).unwrap())
        );
        assert_eq!(
            15,
            move_axis(20, 0, 2, Duration::new(2, 5e7 as u32).unwrap())
        );
        assert_eq!(
            25,
            move_axis(20, 25, 2, Duration::new(5, 5e8 as u32).unwrap())
        );
        assert_eq!(
            18,
            move_axis(20, 18, 2, Duration::new(2, 5e7 as u32).unwrap())
        );
        assert_eq!(
            20,
            move_axis(20, 20, 2, Duration::new(2, 5e7 as u32).unwrap())
        );
    }

    #[test]
    fn test_sim_move_abs() {
        let mut sim = Simulator::new();
        sim.offset = Some(0);
        sim.pos = [[2000, 2000], [100, 0]];
        sim.target = [[2000, 2000], [100, 0]];
        sim.busy = [[false, false], [true, false]];

        let mut port: ZaberConn<Simulator> =
            Port::from_backend(sim, false, false, MaxPacketSize::default());
        let resp = port
            .command_reply((2, "move abs 3000"))
            .unwrap()
            .flag_ok()
            .unwrap();

        assert_eq!(resp.target().device(), 2);
        assert_eq!(resp.target().axis(), 0);
        assert_eq!(port.backend().target[1][0], 3000);

        let resp = port
            .command_reply((1, "move abs 3000"))
            .unwrap()
            .flag_ok()
            .unwrap();

        assert_eq!(resp.target().device(), 1);
        assert_eq!(resp.target().axis(), 0);
        assert_eq!(port.backend().target[0][1], 3000);
    }

    #[test]
    fn test_sim_move_rel() {
        let mut sim = Simulator::new();
        sim.offset = Some(0);
        sim.pos = [[2000, 2000], [100, 0]];
        sim.target = [[2000, 2000], [100, 0]];
        sim.busy = [[false, false], [true, false]];

        let mut port: ZaberConn<Simulator> =
            Port::from_backend(sim, false, false, MaxPacketSize::default());
        let resp = port
            .command_reply((1, "move rel 50"))
            .unwrap()
            .flag_ok()
            .unwrap();

        assert_eq!(resp.target().device(), 1);
        assert_eq!(resp.target().axis(), 0);
        assert_eq!(port.backend().target[0][0], 2050);

        let resp = port
            .command_reply((1, "move rel -100"))
            .unwrap()
            .flag_ok()
            .unwrap();

        assert_eq!(resp.target().device(), 1);
        assert_eq!(resp.target().axis(), 0);
        assert_eq!(port.backend().target[0][0], 1950);
    }

    #[test]
    fn test_sim_get_pos() {
        let mut sim = Simulator::new();
        sim.offset = Some(0);
        sim.pos = [[2000, 2000], [100, 0]];
        sim.target = [[2000, 2000], [100, 0]];
        sim.busy = [[false, false], [true, false]];

        let mut port: ZaberConn<Simulator> =
            Port::from_backend(sim, false, false, MaxPacketSize::default());
        let resp = port
            .command_reply_n("get pos", 2, check::flag_ok())
            .unwrap();

        assert_eq!(resp[0].target().device(), 1);
        assert_eq!(resp[0].target().axis(), 0);
        assert_eq!(resp[0].status(), Status::Idle);
        assert_eq!(resp[0].warning(), "--");
        assert_eq!(resp[0].data(), "2000");

        assert_eq!(resp[1].target().device(), 2);
        assert_eq!(resp[1].target().axis(), 0);
        assert_eq!(resp[1].status(), Status::Idle);
        assert_eq!(resp[1].warning(), "--");
        assert_eq!(resp[1].data(), "100");
    }

    #[test]
    fn test_sim_set_limit() {
        let mut sim = Simulator::new();
        sim.offset = Some(0);
        sim.pos = [[2000, 2000], [100, 0]];
        sim.target = [[2000, 2000], [100, 0]];

        let mut port: ZaberConn<Simulator> =
            Port::from_backend(sim, false, false, MaxPacketSize::default());
        let _ = port
            .command_reply((1, "set limit.max 2500"))
            .unwrap()
            .flag_ok()
            .unwrap();

        let _ = port
            .command_reply((1, "set limit.min 50"))
            .unwrap()
            .flag_ok()
            .unwrap();

        assert_eq!(port.backend().limit[0][0][1], 2500);
        assert_eq!(port.backend().limit[0][1][0], 50);
    }
}
