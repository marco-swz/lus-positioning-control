use std::{borrow::BorrowMut, cell::RefCell, rc::Rc};

use crate::{control::{run, ExecState}, zaber::{get_pos_zaber, move_cross_zaber, move_parallel_zaber}};
use anyhow::Result;
use crossbeam_channel::Receiver;

pub fn init_manual(state: &mut ExecState) -> Result<()> {
    let voltage = Rc::new(RefCell::new(0.));
    let get_voltage = || get_voltage_manual(Rc::clone(&voltage), state.rx_manual.clone());
    let get_pos = || get_pos_zaber(Rc::clone(&zaber_conn));
    let move_parallel = |pos| move_parallel_zaber(Rc::clone(&zaber_conn), pos);
    let move_cross = |pos| move_cross_zaber(Rc::clone(&zaber_conn), pos);

    return run(state, get_voltage, get_pos, move_parallel, move_cross);
}

fn get_voltage_manual(mut voltage: Rc<RefCell<f64>>, rx_manual: Receiver<f64>) -> Result<f64> {
    let c = voltage.borrow_mut(); 
    let Ok(voltage) = rx_manual.try_recv() else {
        return Ok(c.take());
    };
    c.replace(voltage);

    return Ok(voltage);
}

fn get_pos_manual(mut counter: Rc<RefCell<(f64, f64)>>) -> Result<(f64, f64, bool, bool)> {
    let c = counter.borrow_mut(); 
    let f: (f64, f64) = c.take();
    return Ok((f.0, f.1, false, false));
}

fn move_parallel_manual(mut counter: Rc<RefCell<(f64, f64)>>, pos: f64) -> Result<()> {
    let c = counter.borrow_mut(); 
    let f: (f64, f64) = c.take();
    c.replace((pos, f.1));
    return Ok(());
}

fn move_cross_manual(mut counter: Rc<RefCell<(f64, f64)>>, pos: f64) -> Result<()> {
    let c = counter.borrow_mut(); 
    let f: (f64, f64) = c.take();
    c.replace((f.0, pos));
    return Ok(());
}
