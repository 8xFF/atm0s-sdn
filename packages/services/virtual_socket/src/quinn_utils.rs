use std::sync::Arc;

use quinn::{AsyncStdRuntime, Endpoint, EndpointConfig};

use crate::VirtualUdpSocket;

pub fn make_insecure_quinn_server(socket: VirtualUdpSocket) -> Result<Endpoint, std::io::Error> {
    let runtime = Arc::new(AsyncStdRuntime);
    Endpoint::new_with_abstract_socket(EndpointConfig::default(), Some(quinn_plaintext::server_config()), socket, runtime)
}

pub fn make_insecure_quinn_client(socket: VirtualUdpSocket) -> Result<Endpoint, std::io::Error> {
    let runtime = Arc::new(AsyncStdRuntime);
    let mut endpoint = Endpoint::new_with_abstract_socket(EndpointConfig::default(), None, socket, runtime)?;
    endpoint.set_default_client_config(quinn_plaintext::client_config());
    Ok(endpoint)
}
