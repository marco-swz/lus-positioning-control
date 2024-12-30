use std::io;
use std::io::Write;

use chrono::{DateTime, Duration, Local};

#[derive(Debug)]
pub struct Simulator {
    pub pos_coax1: u32,
    pub pos_coax2: u32,
    pub pos_cross: u32,
    pub lockstep: bool,
    pub vel_cross: f64,
    pub vel_coax: f64,
    pub time: DateTime<Local>,
    pub target_coax1: u32,
    pub target_coax2: u32,
    pub target_cross: u32,
    ignored_read_timeout: Option<std::time::Duration>,
	buffer: io::Cursor<Vec<u8>>,
}

impl Simulator {
    pub fn step(&mut self, time_step: Duration) {
        self.pos_coax1 = move_axis(self.pos_coax1, self.target_coax1, self.vel_coax, time_step);
        self.pos_coax2 = move_axis(self.pos_coax2, self.target_coax2, self.vel_coax, time_step);
        self.pos_cross = move_axis(self.pos_cross, self.target_cross, self.vel_cross, time_step);
        self.time = self.time + time_step;
    }
}

impl zproto::backend::Backend for Simulator {
    fn set_read_timeout(&mut self, timeout: Option<std::time::Duration>) -> Result<(), std::io::Error> {
        self.ignored_read_timeout = timeout;
        Ok(())
	}

	fn read_timeout(&self) -> Result<Option<std::time::Duration>, io::Error> {
		Ok(self.ignored_read_timeout)
	}

    fn name(&self) -> Option<String> {
		Some(format!("<simulator 0x{:x}>", std::ptr::from_ref(self) as usize))
    }
}

impl io::Read for Simulator {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.buffer.read(buf)?;
        return Ok(buf.len());
    }
}

impl io::Write for Simulator {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.buffer.get_ref().as_slice() {
            b"/get pos\n" => write!(self.buffer.get_mut(), "\r\n"),
            _ => panic!("unexpected message"),
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

    let mut pos_new = (
        pos as f64
        + vel * time_step.num_seconds() as f64
        + vel * time_step.subsec_nanos() as f64 / 1.0e9
    ) as u32;

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
    use chrono::Duration;
    use super::move_axis;

    #[test]
    fn test_move() {
        assert_eq!(31, move_axis(20, 100, 2., Duration::new(5, 5e8 as u32).unwrap()));
        assert_eq!(15, move_axis(20, 0, 2., Duration::new(2, 5e7 as u32).unwrap()));
        assert_eq!(25, move_axis(20, 25, 2., Duration::new(5, 5e8 as u32).unwrap()));
        assert_eq!(18, move_axis(20, 18, 2., Duration::new(2, 5e7 as u32).unwrap()));
        assert_eq!(20, move_axis(20, 20, 2., Duration::new(2, 5e7 as u32).unwrap()));
    }
}
