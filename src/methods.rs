use std::{net::TcpStream, sync::{Arc, Mutex}};

use opcua::server::{callbacks, prelude::*};
use zproto::binary::{Port, SendCallbacks};

struct CbStop;
impl callbacks::Method for CbStop {
    fn call(
        &mut self,
        _session_id: &NodeId,
        _session_manager: std::sync::Arc<opcua::sync::RwLock<opcua::server::session::SessionManager>>,
        _request: &CallMethodRequest,
    ) -> Result<CallMethodResult, StatusCode> {

        println!("stop");

        return Ok(CallMethodResult{
            status_code: StatusCode::Good,
            input_argument_results: None,
            input_argument_diagnostic_infos: None,
            output_arguments: Some(vec![Variant::String("Ok".into())]),
        });
    }
}


struct CbMoveAbsolute;
impl callbacks::Method for CbMoveAbsolute {
    fn call(
        &mut self,
        _session_id: &NodeId,
        _session_manager: std::sync::Arc<opcua::sync::RwLock<opcua::server::session::SessionManager>>,
        request: &CallMethodRequest,
    ) -> Result<CallMethodResult, StatusCode> {
        let Some(ref args) = request.input_arguments else {
            return Err(StatusCode::BadArgumentsMissing);
        };

        let Some(pos) = args.get(0) else {
            return Err(StatusCode::BadArgumentsMissing);
        };
        let Variant::Double(pos) = pos else {
            return Err(StatusCode::BadInvalidArgument);
        };

        let Some(vel) = args.get(1) else {
            return Err(StatusCode::BadArgumentsMissing);
        };
        let Variant::Double(vel) = vel else {
            return Err(StatusCode::BadInvalidArgument);
        };

        let Some(acc) = args.get(2) else {
            return Err(StatusCode::BadArgumentsMissing);
        };
        let Variant::Double(acc) = acc else {
            return Err(StatusCode::BadInvalidArgument);
        };

        println!("move_absolute: {}, {}, {}", pos, vel, acc);

        return Ok(CallMethodResult{
            status_code: StatusCode::Good,
            input_argument_results: None,
            input_argument_diagnostic_infos: None,
            output_arguments: Some(vec![Variant::String("Ok".into())]),
        });
    }
}


struct CbMoveVelocity;
impl callbacks::Method for CbMoveVelocity {
    fn call(
        &mut self,
        _session_id: &NodeId,
        _session_manager: std::sync::Arc<opcua::sync::RwLock<opcua::server::session::SessionManager>>,
        request: &CallMethodRequest,
    ) -> Result<CallMethodResult, StatusCode> {
        let Some(ref args) = request.input_arguments else {
            return Err(StatusCode::BadArgumentsMissing);
        };

        let Some(pos) = args.get(0) else {
            return Err(StatusCode::BadArgumentsMissing);
        };
        let Variant::Double(pos) = pos else {
            return Err(StatusCode::BadInvalidArgument);
        };

        let Some(vel) = args.get(1) else {
            return Err(StatusCode::BadArgumentsMissing);
        };
        let Variant::Double(vel) = vel else {
            return Err(StatusCode::BadInvalidArgument);
        };

        println!("move_velocity: {}, {}", pos, vel);

        return Ok(CallMethodResult{
            status_code: StatusCode::Good,
            input_argument_results: None,
            input_argument_diagnostic_infos: None,
            output_arguments: Some(vec![Variant::String("Ok".into())]),
        });
    }
}

pub fn add_methods(server: &mut Server, ns: u16, node_id: NodeId, zaber: Arc<Mutex<Port<TcpStream, SendCallbacks>>>) {
    let address_space = server.address_space();
    let mut address_space = address_space.write();

    let node_stop = NodeId::new(ns, "stop");
    MethodBuilder::new(&node_stop, "stop", "stop")
        .component_of(node_id.clone())
        .input_args(
            &mut address_space,
            &[],
        )
        .output_args(&mut address_space, &[("response status", DataTypeId::String).into()])
        .callback(Box::new(CbStop))
        .insert(&mut address_space);

    let node_move_abs = NodeId::new(ns, "move_absolute");
    MethodBuilder::new(&node_move_abs, "move_absolute", "move_absolute")
        .component_of(node_id.clone())
        .input_args(
            &mut address_space,
            &[
                ("absolute position [mm]", DataTypeId::Double).into(),
                ("velocity [mm/s]", DataTypeId::Double).into(),
                ("acceleration [mm/s^2]", DataTypeId::Double).into(),
            ],
        )
        .output_args(&mut address_space, &[("response status", DataTypeId::String).into()])
        .callback(Box::new(CbMoveAbsolute))
        .insert(&mut address_space);

    let node_move_rel = NodeId::new(ns, "move_relative");
    MethodBuilder::new(&node_move_rel, "move_relative", "move_relative")
        .component_of(node_id.clone())
        .input_args(
            &mut address_space,
            &[
                ("relative position [mm]", DataTypeId::Double).into(),
                ("velocity [mm/s]", DataTypeId::Double).into(),
                ("acceleration [mm/s^2]", DataTypeId::Double).into(),
            ],
        )
        .output_args(&mut address_space, &[("response status", DataTypeId::String).into()])
        .callback(Box::new(CbMoveAbsolute))
        .insert(&mut address_space);

    let node_move_vel = NodeId::new(ns, "move_velocity");
    MethodBuilder::new(&node_move_vel, "move_velocity", "move_velocity")
        .component_of(node_id)
        .input_args(
            &mut address_space,
            &[
                ("velocity [mm/s]", DataTypeId::Double).into(),
                ("acceleration [mm/s^2]", DataTypeId::Double).into(),
            ],
        )
        .output_args(&mut address_space, &[("response status", DataTypeId::String).into()])
        .callback(Box::new(CbMoveVelocity))
        .insert(&mut address_space);
}

