use std::{borrow::BorrowMut, cell::RefCell, rc::Rc};

use crate::{control::run, utils::ExecState};
use anyhow::Result;

pub fn init_ramp(state: &mut ExecState) -> Result<()> {
    let counter_true = Rc::new(RefCell::new(0.));
    let counter = Rc::new(RefCell::new((0., 0.)));
    let get_voltage = || get_voltage_ramp(Rc::clone(&counter_true));
    let get_pos = || get_pos_ramp(Rc::clone(&counter));
    let move_parallel = |pos| move_parallel_ramp(Rc::clone(&counter), pos);
    let move_cross = |pos| move_cross_ramp(Rc::clone(&counter), pos);

    return run(state, get_voltage, get_pos, move_parallel, move_cross);
}

fn get_voltage_ramp(mut counter: Rc<RefCell<f64>>) -> Result<f64> {
    let c = counter.borrow_mut();
    let mut f: f64 = c.take() + 1.;
    if f > 100. {
        f = 0.;
    }
    c.replace(f);

    return Ok(f);
}

fn get_pos_ramp(mut counter: Rc<RefCell<(f64, f64)>>) -> Result<(f64, f64, bool, bool)> {
    let c = counter.borrow_mut();
    let f: (f64, f64) = c.take();
    return Ok((f.0, f.1, false, false));
}

fn move_parallel_ramp(mut counter: Rc<RefCell<(f64, f64)>>, pos: f64) -> Result<()> {
    let c = counter.borrow_mut();
    let f: (f64, f64) = c.take();
    c.replace((pos, f.1));
    return Ok(());
}

fn move_cross_ramp(mut counter: Rc<RefCell<(f64, f64)>>, pos: f64) -> Result<()> {
    let c = counter.borrow_mut();
    let f: (f64, f64) = c.take();
    c.replace((f.0, pos));
    return Ok(());
}
