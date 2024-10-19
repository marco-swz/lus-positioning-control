use std::sync::{Arc, Condvar, Mutex};

use crossbeam_queue::ArrayQueue;
use opcua::server::prelude::*;

mod methods;
use methods::{add_axis_methods, add_methods};

mod control;
use control::{connect_state, ControlState, ExecState, SharedState, StateChannel, Config};

fn add_axis_variables(
    server: &mut Server,
    ns: u16,
    zaber: StateChannel,
) -> (NodeId, NodeId) {
    let address_space = server.address_space();

    let node_position_cross = NodeId::new(ns, "position");
    let node_busy_cross = NodeId::new(ns, "busy");
    let node_position_parallel = NodeId::new(ns, "position");
    let node_busy_parallel = NodeId::new(ns, "busy");
    let node_status = NodeId::new(ns, "status");

    let root_id = NodeId::objects_folder_id();

    let folders = {
        let mut address_space = address_space.write();

        let folder_cross_id = address_space
            .add_folder("cross-slide", "cross-slide", &root_id)
            .unwrap();
        let _ = address_space.add_variables(
            vec![
                Variable::new(&node_position_cross, "position", "position [mm]", 0 as f64),
                Variable::new(&node_busy_cross, "busy", "busy", false),
            ],
            &folder_cross_id,
        );

        let folder_parallel_id = address_space
            .add_folder("parallel-slide", "parallel-slide", &root_id)
            .unwrap();
        let _ = address_space.add_variables(
            vec![
                Variable::new(&node_position_parallel, "position", "position [mm]", 0 as f64),
                Variable::new(&node_busy_parallel, "busy", "busy", false),
            ],
            &folder_parallel_id,
        );

        address_space.add_variables(
            vec![
                Variable::new(&node_status, "status", "status", UAString::from("Init")),
            ], &root_id);

        (folder_cross_id, folder_parallel_id)
    };

    server.add_polling_action(1000, move || {
        let Some(zaber_state) = zaber.pop() else {
            return;
        };

        let now = DateTime::now();

        let mut address_space = address_space.write();
        let _ = address_space.set_variable_value(
            node_position_parallel.clone(),
            zaber_state.position_parallel,
            &now,
            &now,
        );
        let _ = address_space.set_variable_value(
            node_busy_parallel.clone(),
            zaber_state.busy_parallel,
            &now,
            &now,
        );
        let _ = address_space.set_variable_value(
            node_position_cross.clone(),
            zaber_state.position_cross,
            &now,
            &now,
        );
        let _ = address_space.set_variable_value(
            node_busy_cross.clone(),
            zaber_state.busy_cross,
            &now,
            &now,
        );
        let _ = address_space.set_variable_value(
            node_status.clone(),
            format!("{:?}", zaber_state.control_state),
            &now,
            &now,
        );
    });

    return folders;
}

fn run_opcua(zaber_state: StateChannel) {
    let mut server: Server = ServerBuilder::new()
        .application_name("zaber-opcua")
        .application_uri("urn:zaber-opcua")
        .discovery_urls(vec!["/".into()])
        .endpoint(
            "none",
            ServerEndpoint::new_none("/", &[ANONYMOUS_USER_TOKEN_ID.into()]),
        )
        .trust_client_certs()
        .multi_threaded_executor()
        .create_sample_keypair(false)
        .discovery_server_url(None)
        .host_and_port(hostname().unwrap(), 4343)
        .server()
        .unwrap();

    let ns = {
        let address_space = server.address_space();
        let mut address_space = address_space.write();
        address_space.register_namespace("urn:zaber-opcua").unwrap()
    };

    //add_methods(&mut server, ns, zaber_state);

    let _node_ids = add_axis_variables(&mut server, ns, Arc::clone(&zaber_state));
    //add_axis_methods(&mut server, ns, node_id, zaber, 1);

    server.run();
}

fn main() {
    let state = ExecState{
        shared: SharedState {
            voltage_gleeble: 0.,
            position_cross: 0.,
            position_parallel: 0.,
            busy_cross: false,
            busy_parallel: false,
            control_state: ControlState::PreConnect,
            error: None,
        },
        config: Config{
            cycle_time_ms: 1000,
            voltage_min: 5.,
            voltage_max: 100.,
        },
        out_channel: Arc::new(ArrayQueue::new(1)),
        stop_channel: Arc::new((Mutex::new(false), Condvar::new())),
    };


    let _ = state.out_channel.force_push(state.shared.clone());

    let queue_clone = Arc::clone(&state.out_channel);
    std::thread::spawn(|| run_opcua(queue_clone));

    connect_state(state);
}
