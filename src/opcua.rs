use std::path::PathBuf;
use std::sync::Arc;

use opcua::server::state::ServerState;
use opcua::{server::prelude::*, sync::RwLock};

use crate::utils::StateChannel;
use crate::zaber::steps_to_mm;

fn add_axis_variables(server: &mut Server, ns: u16, zaber: StateChannel) {
    let address_space = server.address_space();

    let node_position_cross = NodeId::new(ns, "position_cross");
    let node_busy_cross = NodeId::new(ns, "busy_cross");
    let node_position_coax = NodeId::new(ns, "position_coax");
    let node_busy_coax = NodeId::new(ns, "busy_coax");
    let node_status = NodeId::new(ns, "status");

    let root_id = NodeId::objects_folder_id();

    {
        let mut address_space = address_space.write();

        let folder_cross_id = address_space
            .add_folder("cross-slide", "cross-slide", &root_id)
            .unwrap();

        VariableBuilder::new(&node_position_cross, "position", "position [mm]")
            .value(0.)
            .data_type(DataTypeId::Double)
            .organized_by(&folder_cross_id)
            .insert(&mut address_space);

        VariableBuilder::new(&node_busy_cross, "busy", "busy")
            .data_type(DataTypeId::Boolean)
            .organized_by(&folder_cross_id)
            .value(false)
            .insert(&mut address_space);

        let folder_coax_id = address_space
            .add_folder("coax-slide", "coax-slide", &root_id)
            .unwrap();

        VariableBuilder::new(&node_position_coax, "position", "position [mm]")
            .value(0.)
            .data_type(DataTypeId::Double)
            .organized_by(&folder_coax_id)
            .insert(&mut address_space);

        VariableBuilder::new(&node_busy_coax, "busy", "busy")
            .data_type(DataTypeId::Boolean)
            .organized_by(&folder_coax_id)
            .value(false)
            .insert(&mut address_space);

        let folder_general_id = address_space
            .add_folder("general", "general", &root_id)
            .unwrap();

        VariableBuilder::new(&node_status, "status", "status")
            .value(UAString::from("Init"))
            .data_type(DataTypeId::String)
            .organized_by(&folder_general_id)
            .insert(&mut address_space);
    };

    server.add_polling_action(1000, move || {
        let Ok(zaber_state) = zaber.try_read() else {
            return;
        };

        let now = DateTime::now();

        let mut address_space = address_space.write();
        for i in 0..2 {
            let _ = address_space.set_variable_value(
                node_position_coax.clone(),
                steps_to_mm(zaber_state.position[i]),
                &now,
                &now,
            );
            let _ = address_space.set_variable_value(
                node_busy_coax.clone(),
                zaber_state.is_busy[i],
                &now,
                &now,
            );
        }

        let _ = address_space.set_variable_value(
            node_status.clone(),
            format!("{:?}", zaber_state.control_state),
            &now,
            &now,
        );
    });
}

pub fn run_opcua(zaber_state: StateChannel, config_path: PathBuf) -> Arc<RwLock<ServerState>> {
    tracing::debug!("Start opcua server");

    let config: Result<ServerConfig, ()> = ServerConfig::load(&config_path);

    let mut server = match config {
        Ok(config) => Server::from(config),
        Err(_) => {
            tracing::error!("Opcua config error -> using default");
            ServerBuilder::new()
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
                .unwrap()
        }
    };

    let ns = {
        let address_space = server.address_space();
        let mut address_space = address_space.write();
        address_space.register_namespace("urn:zaber-opcua").unwrap()
    };

    add_axis_variables(&mut server, ns, Arc::clone(&zaber_state));

    let state = server.server_state();
    std::thread::spawn(|| server.run());

    return state;
}
