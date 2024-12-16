use anyhow::Result;
use chrono::Local;
use crate::utils::{Backend, ControlStatus, ExecState};

use crate::zaber::steps_to_mm;
use crate::{ramp::init_ramp, zaber::{init_zaber, MAX_POS}};

pub fn init(state: &mut ExecState) -> Result<()> {
    let backend = {
        state.config.read().unwrap().backend.clone()
    };

    state.shared.error = None;
    if let Ok(mut out) = state.out_channel.try_write() {
        *out = state.shared.clone();
        drop(out);
    }

    tracing::debug!("Init control with backend {:?}", &backend);
    return match backend {
        Backend::Zaber | Backend::Manual => init_zaber(state),
        Backend::Ramp => init_ramp(state),
    };
}

pub fn run(
    state: &mut ExecState,
    mut get_voltage: impl FnMut() -> Result<f64>,
    mut get_pos: impl FnMut() -> Result<(f64, f64, bool, bool)>,
    mut move_coax: impl FnMut(f64) -> Result<()>,
    _move_cross: impl FnMut(f64) -> Result<()>,
) -> Result<()> {
    state.shared.control_state = ControlStatus::Running;

    let config = state.config.read().unwrap();
    let voltage_max = config.voltage_max;
    let voltage_min = config.voltage_min;
    let cycle_time = config.cycle_time;
    drop(config);

    tracing::info!("Starting control loop");
    loop {
        let voltage_gleeble = get_voltage()?;
        tracing::debug!("Voltage reading {voltage_gleeble}");
        state.shared.voltage_gleeble = voltage_gleeble;

        let target_position_coax = steps_to_mm(MAX_POS) / (voltage_max - voltage_min) * (voltage_gleeble - voltage_min);

        let (pos_coax, pos_cross, busy_coax, busy_cross) = get_pos()?;
        state.shared.position_coax = pos_coax;
        state.shared.position_cross = pos_cross;
        state.shared.busy_coax = busy_coax;
        state.shared.busy_cross = busy_cross;
        state.shared.timestamp = Local::now();

        tracing::debug!("Position coax: target={target_position_coax} actual={pos_coax}");
        if target_position_coax != pos_coax {
            move_coax(target_position_coax)?;
        }

        if let Ok(mut out) = state.out_channel.try_write() {
            *out = state.shared.clone();
            drop(out);
        }

        if let Ok(_) = state.rx_stop.recv_timeout(cycle_time) {
            break;
        }
    }

    tracing::info!("Control loop stopped");
    state.shared.control_state = ControlStatus::Stopped;
    state.shared.timestamp = Local::now();
    let mut out = state.out_channel.write().unwrap();
    *out = state.shared.clone();
    drop(out);
    return Ok(());
}

#[cfg(test)]
mod tests {
    /*
    use zproto::ascii::Port;

    use super::*;


    fn prepare_state() -> ExecState {
        let state = ExecState {
            shared: SharedState {
                voltage_gleeble: 0.,
                position_cross: 0.,
                position_coax: 0.,
                busy_cross: false,
                busy_coax: false,
                control_state: ControlState::Init,
                error: None,
            },
            config: Config {
                cycle_time_ms: 1000,
                voltage_min: 5.,
                voltage_max: 100.,
                serial_device: "".to_string(),
            },
            out_channel: Arc::new(ArrayQueue::new(1)),
            stop_channel: Arc::new((Mutex::new(false), Condvar::new())),
        };

        return state;
    }

    #[test]
    /// Single loop with stop command
    fn test_run_stop() {
        let mut port = Port::open_mock();
        let backend = port.backend_mut();
        backend.set_reply_callback(|buffer, msg| {
            buffer.extend_from_slice(match msg {
                b"/1 io get ai 1\n" => b"@01 0 OK BUSY -- 5.5\r\n",
                b"/1 lockstep 1 move abs 5\n" => b"@01 0 OK BUSY -- 0\r\n",
                b"/get pos\n" => b"@01 0 OK BUSY -- 20\r\n@02 0 OK IDLE -- 10.1\r\n",
                e => panic!("unexpected message: {:?}", e),
            })
        });

        //// /io get ai 1
        //backend.push(b"@01 0 OK BUSY -- 5.5\r\n");
        //// /move abs
        //backend.push(b"@01 0 OK BUSY -- 0\r\n");
        //// /get pos
        //backend.push(b"@01 0 OK BUSY -- 20\r\n");
        //backend.push(b"@02 0 OK IDLE -- 10.1\r\n");

        let mut state = prepare_state();
        let _ = state.out_channel.force_push(state.shared.clone());

        {
            let (lock, cvar) = &*state.stop_channel;
            let mut stop = lock.lock().unwrap();
            *stop = true;
            cvar.notify_one();
        }

        assert!(run(&mut state, &mut port).is_ok());

        assert_eq!(
            state.shared,
            SharedState {
                voltage_gleeble: 5.5,
                position_coax: 20.,
                position_cross: 10.1,
                busy_coax: true,
                busy_cross: false,
                control_state: ControlState::Stopped,
                error: None,
            }
        );
    }

    #[test]
    /// Two loops with timeout (= disconnect)
    fn test_run_disconnect() {
        let mut port = Port::open_mock();
        let backend = port.backend_mut();
        // /io get ai 1
        backend.push(b"@01 0 OK BUSY -- 5.5\r\n");
        // /move abs
        backend.push(b"@01 0 OK BUSY -- 0\r\n");
        // /get pos
        backend.push(b"@01 0 OK BUSY -- 20\r\n");
        backend.push(b"@02 0 OK IDLE -- 10.1\r\n");

        // /io get ai 1
        backend.push(b"@01 0 OK BUSY -- 6.5\r\n");
        // /move abs
        backend.push(b"@01 0 OK BUSY -- 0\r\n");
        // /get pos
        backend.push(b"@01 0 OK IDLE -- 20.1\r\n");
        // Last message missing -> timeout

        let mut state = prepare_state();
        let _ = state.out_channel.force_push(state.shared.clone());

        assert!(run(&mut state, &mut port).is_err());

        assert_eq!(
            state.shared,
            SharedState {
                voltage_gleeble: 6.5,
                position_coax: 20.1,
                position_cross: 10.1,
                busy_coax: false,
                busy_cross: false,
                control_state: ControlState::Running,
                error: None,
            }
        );
    }

    #[test]
    /// Error in reply
    fn test_run_reply() {
        let mut port = Port::open_mock();
        let backend = port.backend_mut();
        // /io get ai 1
        backend.push(b"@01 0 OK BUSY -- 5.5\r\n");
        // /move abs
        backend.push(b"@01 0 OK BUSY -- 0\r\n");
        // /get pos
        backend.push(b"@01 0 OK BUSY -- 20\r\n");
        backend.push(b"@02 0 OK IDLE -- 10.1\r\n");

        // /io get ai 1
        backend.push(b"@01 0 OK BUSY -- 6.5\r\n");
        // /move abs
        backend.push(b"@01 0 OK BUSY -- 0\r\n");
        // /get pos
        backend.push(b"@01 0 OK IDLE -- 20.1\r\n");
        // Last message missing -> timeout

        let mut state = prepare_state();
        let _ = state.out_channel.force_push(state.shared.clone());

        assert!(run(&mut state, &mut port).is_err());

        assert_eq!(
            state.shared,
            SharedState {
                voltage_gleeble: 6.5,
                position_coax: 20.1,
                position_cross: 10.1,
                busy_coax: false,
                busy_cross: false,
                control_state: ControlState::Running,
                error: None,
            }
        );
    }

    #[test]
    /// Error in reply
    fn test_run_reply_err() {
        let mut port = Port::open_mock();
        let backend = port.backend_mut();
        // /io get ai 1
        backend.push(b"@01 0 OK BUSY -- 5.5\r\n");
        // /move abs
        backend.push(b"@01 0 RJ BUSY WR 0\r\n");
        // /get pos
        backend.push(b"@01 0 OK BUSY -- 20\r\n");
        backend.push(b"@02 0 OK IDLE -- 10.1\r\n");

        let mut state = prepare_state();
        let _ = state.out_channel.force_push(state.shared.clone());

        assert!(run(&mut state, &mut port).is_err());

        assert_eq!(
            state.shared,
            SharedState {
                voltage_gleeble: 5.5,
                position_coax: 0.,
                position_cross: 0.,
                busy_coax: false,
                busy_cross: false,
                control_state: ControlState::Running,
                error: None,
            }
        );
    }
    */
}
