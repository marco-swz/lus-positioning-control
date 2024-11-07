use std::sync::{Arc, Mutex, MutexGuard};

use opcua::server::{callbacks, prelude::*};
use zproto::{ascii::port::SendPort, backend::Serial};

type Zaber = SendPort<'static, Serial>;

struct ZaberCallback {
    zaber: Arc<Mutex<Option<SendPort<'static, Serial>>>>,
    device_id: u8,
    action: fn(&mut Zaber, &CallMethodRequest, u8) -> (StatusCode, String),
}
impl callbacks::Method for ZaberCallback {
    fn call(
        &mut self,
        _session_id: &NodeId,
        _session_manager: std::sync::Arc<
            opcua::sync::RwLock<opcua::server::session::SessionManager>,
        >,
        request: &CallMethodRequest,
    ) -> Result<CallMethodResult, StatusCode> {
        let Ok(mut zaber) = self.zaber.lock() else {
            return Err(StatusCode::BadInternalError);
        };

        let Some(ref mut zaber) = *zaber else {
            return Err(StatusCode::BadInternalError);
        };

        let (status_code, status_text) = (self.action)(zaber, request, self.device_id);

        let _ = zaber;

        return Ok(CallMethodResult {
            status_code,
            input_argument_results: None,
            input_argument_diagnostic_infos: None,
            output_arguments: Some(vec![Variant::String(status_text.into())]),
        });
    }
}

fn handle_stop(
    zaber: &mut Zaber,
    _request: &CallMethodRequest,
    device_id: u8,
) -> (StatusCode, String) {
    let (status_code, status_text) = match zaber.command_reply((device_id, "stop")) {
        Ok(_) => (StatusCode::Good, "Ok".into()),
        Err(e) => (StatusCode::BadInternalError, e.to_string()),
    };
    return (status_code, status_text);
}

fn handle_move_absolute(
    zaber: &mut Zaber,
    request: &CallMethodRequest,
    device_id: u8,
) -> (StatusCode, String) {
    let Some(ref args) = request.input_arguments else {
        return (StatusCode::BadArgumentsMissing, "Missing input arguments".into());
    };

    let Some(pos) = args.get(0) else {
        return (StatusCode::BadArgumentsMissing, "Missing position argument".into());
    };
    let Variant::Double(pos) = pos else {
        return (StatusCode::BadInvalidArgument, "Cannot convert position into a number".into());
    };

    let Some(vel) = args.get(1) else {
        return (StatusCode::BadArgumentsMissing, "Missing velocity argument".into());
    };
    let Variant::Double(vel) = vel else {
        return (StatusCode::BadInvalidArgument, "Cannot convert velocity into a number".into());
    };

    let Some(acc) = args.get(2) else {
        return (StatusCode::BadArgumentsMissing, "Missing acceleration argument".into());
    };
    let Variant::Double(acc) = acc else {
        return (StatusCode::BadInvalidArgument, "Cannot convert acceleration into a number".into());
    };

    let cmd = format!("move abs {} {} {}", pos, vel, acc);
    let (status_code, status_text) = match zaber.command_reply((device_id, cmd)) {
        Ok(_) => (StatusCode::Good, "Ok".into()),
        Err(e) => (StatusCode::BadInternalError, e.to_string()),
    };
    return (status_code, status_text);
}

fn handle_move_relative(
    zaber: &mut Zaber,
    request: &CallMethodRequest,
    device_id: u8,
) -> (StatusCode, String) {
    let Some(ref args) = request.input_arguments else {
        return (StatusCode::BadArgumentsMissing, "Missing input arguments".into());
    };

    let Some(pos) = args.get(0) else {
        return (StatusCode::BadArgumentsMissing, "Missing position argument".into());
    };
    let Variant::Double(pos) = pos else {
        return (StatusCode::BadInvalidArgument, "Cannot convert position into a number".into());
    };

    let Some(vel) = args.get(1) else {
        return (StatusCode::BadArgumentsMissing, "Missing velocity argument".into());
    };
    let Variant::Double(vel) = vel else {
        return (StatusCode::BadInvalidArgument, "Cannot convert velocity into a number".into());
    };

    let Some(acc) = args.get(2) else {
        return (StatusCode::BadArgumentsMissing, "Missing acceleration argument".into());
    };
    let Variant::Double(acc) = acc else {
        return (StatusCode::BadInvalidArgument, "Cannot convert acceleration into a number".into());
    };

    let cmd = format!("move rel {} {} {}", pos, vel, acc);
    let (status_code, status_text) = match zaber.command_reply((device_id, cmd)) {
        Ok(_) => (StatusCode::Good, "Ok".into()),
        Err(e) => (StatusCode::BadInternalError, e.to_string()),
    };
    return (status_code, status_text);
}

fn handle_move_velocity(
    zaber: &mut Zaber,
    request: &CallMethodRequest,
    device_id: u8,
) -> (StatusCode, String) {
    let Some(ref args) = request.input_arguments else {
        return (StatusCode::BadArgumentsMissing, "Missing input arguments".into());
    };

    let Some(pos) = args.get(0) else {
        return (StatusCode::BadArgumentsMissing, "Missing position argument".into());
    };
    let Variant::Double(pos) = pos else {
        return (StatusCode::BadInvalidArgument, "Cannot convert position into a number".into());
    };

    let Some(vel) = args.get(1) else {
        return (StatusCode::BadArgumentsMissing, "Missing velocity argument".into());
    };
    let Variant::Double(vel) = vel else {
        return (StatusCode::BadInvalidArgument, "Cannot convert velocity into a number".into());
    };

    println!("move_velocity: {}, {}", pos, vel);
    let cmd = format!("move vel {} {}", pos, vel);
    let (status_code, status_text) = match zaber.command_reply((device_id, cmd)) {
        Ok(_) => (StatusCode::Good, "Ok".into()),
        Err(e) => (StatusCode::BadInternalError, e.to_string()),
    };
    return (status_code, status_text);
}

fn handle_command(
    zaber: &mut Zaber,
    request: &CallMethodRequest,
    _device_id: u8,
) -> (StatusCode, String) {
    let Some(ref args) = request.input_arguments else {
        return (StatusCode::BadArgumentsMissing, "Missing input arguments".into());
    };

    let Some(cmd) = args.get(0) else {
        return (StatusCode::BadArgumentsMissing, "Missing command argument".into());
    };

    println!("command: {}", cmd);
    let (status_code, status_text) = match zaber.command_reply(cmd.to_string()) {
        Ok(_) => (StatusCode::Good, "Ok".into()),
        Err(e) => (StatusCode::BadInternalError, e.to_string()),
    };
    return (status_code, status_text);
}

pub fn add_methods(
    server: &mut Server,
    ns: u16,
    zaber: Arc<Mutex<Option<SendPort<'static, Serial>>>>,
) {
    let address_space = server.address_space();
    let mut address_space = address_space.write();

    MethodBuilder::new(&NodeId::new(ns, "command"), "command", "command")
        .input_args(&mut address_space,
            &[
                ("Zaber command", DataTypeId::String).into(),
            ]
        )
        .output_args(
            &mut address_space,
            &[("response status", DataTypeId::String).into()],
        )
        .callback(Box::new(ZaberCallback{
            zaber: Arc::clone(&zaber),
            device_id: 0,
            action: handle_command,
        }))
        .insert(&mut address_space);
}

pub fn add_axis_methods(
    server: &mut Server,
    ns: u16,
    node_id: NodeId,
    zaber: Arc<Mutex<Option<SendPort<'static, Serial>>>>,
    device_id: u8,
) {
    let address_space = server.address_space();
    let mut address_space = address_space.write();

    MethodBuilder::new(&NodeId::new(ns, "stop"), "stop", "stop")
        .component_of(node_id.clone())
        .input_args(&mut address_space, &[])
        .output_args(
            &mut address_space,
            &[("response status", DataTypeId::String).into()],
        )
        .callback(Box::new(ZaberCallback{
            zaber: Arc::clone(&zaber),
            device_id,
            action: handle_stop,
        }))
        .insert(&mut address_space);

    MethodBuilder::new(&NodeId::new(ns, "move_absolute"), "move_absolute", "move_absolute")
        .component_of(node_id.clone())
        .input_args(
            &mut address_space,
            &[
                ("absolute position [mm]", DataTypeId::Double).into(),
                ("velocity [mm/s]", DataTypeId::Double).into(),
                ("acceleration [mm/s^2]", DataTypeId::Double).into(),
            ],
        )
        .output_args(
            &mut address_space,
            &[("response status", DataTypeId::String).into()],
        )
        .callback(Box::new(ZaberCallback{
            zaber: Arc::clone(&zaber),
            device_id,
            action: handle_move_absolute,
        }))
        .insert(&mut address_space);

    MethodBuilder::new(&NodeId::new(ns, "move_relative"), "move_relative", "move_relative")
        .component_of(node_id.clone())
        .input_args(
            &mut address_space,
            &[
                ("relative position [mm]", DataTypeId::Double).into(),
                ("velocity [mm/s]", DataTypeId::Double).into(),
                ("acceleration [mm/s^2]", DataTypeId::Double).into(),
            ],
        )
        .output_args(
            &mut address_space,
            &[("response status", DataTypeId::String).into()],
        )
        .callback(Box::new(ZaberCallback{
            zaber: Arc::clone(&zaber),
            device_id,
            action: handle_move_relative,
        }))
        .insert(&mut address_space);

    MethodBuilder::new(&NodeId::new(ns, "move_velocity"), "move_velocity", "move_velocity")
        .component_of(node_id)
        .input_args(
            &mut address_space,
            &[
                ("velocity [mm/s]", DataTypeId::Double).into(),
                ("acceleration [mm/s^2]", DataTypeId::Double).into(),
            ],
        )
        .output_args(
            &mut address_space,
            &[("response status", DataTypeId::String).into()],
        )
        .callback(Box::new(ZaberCallback{
            zaber: Arc::clone(&zaber),
            device_id,
            action: handle_move_velocity,
        }))
        .insert(&mut address_space);
}
