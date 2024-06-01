use std::{net::TcpStream, sync::{Arc, Mutex}};

use opcua::server::prelude::*;
use zproto::binary::{command::RETURN_CURRENT_POSITION, Message, Port};

mod methods;
use methods::add_methods;

fn add_variables(server: &mut Server, ns: u16, name: &str, zaber: Arc<Mutex<Port<TcpStream>>>) -> NodeId {
    let address_space = server.address_space();

    let node_position = NodeId::new(ns, "position");
    let node_status = NodeId::new(ns, "status");
    let node_busy = NodeId::new(ns, "busy");

    let folder_id = {
        let mut address_space = address_space.write();

        let folder_id = address_space
            .add_folder(name, name, &NodeId::objects_folder_id())
            .unwrap();

        let _ = address_space.add_variables(
            vec![
                Variable::new(&node_position, "position", "position [mm]", 0 as f64),
                Variable::new(&node_status, "status", "status", UAString::from("Inii")),
                Variable::new(&node_busy, "busy", "busy", false),
            ],
            &folder_id,
        );

        folder_id
    };

    let zaber = Arc::clone(&zaber);
    server.add_polling_action(1000, move || {
        let mut address_space = address_space.write();

        let mut zaber = zaber.lock().unwrap();

        let now = DateTime::now();
        let status = match zaber.tx_recv((0, RETURN_CURRENT_POSITION)) {
            Ok(resp) => match resp.data() {
                Ok(pos) => {
                    let _ = address_space.set_variable_value(node_position.clone(), pos, &now, &now);
                    "Ok".into()
                }
                Err(e) => e.to_string(),
            },
            Err(e) => e.to_string(),
        };


        let _ = address_space.set_variable_value(node_busy.clone(), true, &now, &now);
        let _ = address_space.set_variable_value(node_status.clone(), status, &now, &now);
    });

    return folder_id;
}

fn main() {
    let mut server: Server = ServerBuilder::new()
        .application_name("zaber-opcua")
        .application_uri("urn:zaber-opcua")
        .discovery_urls(vec!["/".into()])
        .endpoint("none", ServerEndpoint::new_none("/", &[ANONYMOUS_USER_TOKEN_ID.into()]))
        .trust_client_certs()
        .multi_threaded_executor()
        .create_sample_keypair(false)
        .discovery_server_url(None)
        .host_and_port(hostname().unwrap(), 4343)
        .server()
        .unwrap();

    //let mut zaber = ascii::Port::open_serial("/dev/ttyACM0").unwrap();
    let mut zaber = Port::open_tcp("/dev/ttyACM0").unwrap();
    let zaber = Arc::new(Mutex::new(zaber));

    let ns = {
        let address_space = server.address_space();
        let mut address_space = address_space.write();
        address_space
            .register_namespace("urn:zaber-opcua")
            .unwrap()
    };

    let node_id = add_variables(&mut server, ns, "cross-slide", zaber);
    add_methods(&mut server, ns, node_id, zaber);


    server.run();
}
