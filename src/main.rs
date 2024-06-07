use std::{
    net::TcpStream,
    sync::{Arc, Mutex},
};

use opcua::server::prelude::*;
use zproto::binary::{
    command::{RETURN_CURRENT_POSITION, RETURN_STATUS},
    Port, SendCallbacks,
};

mod methods;
use methods::add_methods;

fn add_variables(
    server: &mut Server,
    ns: u16,
    name: &str,
    zaber: Arc<Mutex<Port<'static, TcpStream, SendCallbacks<'static>>>>,
) -> NodeId {
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

    server.add_polling_action(1000, move || {
        let mut zaber = zaber.lock().unwrap();
        let mut pos = 0;
        let mut busy = false;

        let now = DateTime::now();
        let status = match zaber.tx_recv((0, RETURN_CURRENT_POSITION)) {
            Ok(resp) => match resp.data() {
                Ok(p) => {
                    pos = p;
                    "Ok".into()
                }
                Err(e) => e.to_string(),
            },
            Err(e) => e.to_string(),
        };
        let status = match zaber.tx_recv((0, RETURN_STATUS)) {
            Ok(resp) => match resp.data() {
                Ok(p) => {
                    //busy = resp;
                    "Ok".into()
                }
                Err(e) => e.to_string(),
            },
            Err(e) => e.to_string(),
        };

        let mut address_space = address_space.write();
        let _ = address_space.set_variable_value(node_position.clone(), pos, &now, &now);
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

    //let mut zaber = ascii::Port::open_serial("/dev/ttyACM0").unwrap();
    let zaber = Port::open_tcp("/dev/ttyACM0").unwrap();
    let zaber = zaber.try_into_send().unwrap();
    let zaber = Arc::new(Mutex::new(zaber));

    let ns = {
        let address_space = server.address_space();
        let mut address_space = address_space.write();
        address_space.register_namespace("urn:zaber-opcua").unwrap()
    };

    let node_id = add_variables(&mut server, ns, "cross-slide", Arc::clone(&zaber));
    add_methods(&mut server, ns, node_id, zaber);

    server.run();
}
