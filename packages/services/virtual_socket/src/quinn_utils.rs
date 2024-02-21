use std::sync::Arc;

use atm0s_sdn_utils::error_handle::ErrorUtils;
use quinn::{AsyncStdRuntime, Endpoint, EndpointConfig};

use crate::VirtualUdpSocket;

pub fn make_insecure_quinn_server(socket: VirtualUdpSocket) -> Result<Endpoint, std::io::Error> {
    let runtime = Arc::new(AsyncStdRuntime);
    let mut config = EndpointConfig::default();
    config.max_udp_payload_size(1500).print_error("Should config quinn server max_size to 1500");
    Endpoint::new_with_abstract_socket(config, Some(quinn_plaintext::server_config()), socket, runtime)
}

pub fn make_insecure_quinn_client(socket: VirtualUdpSocket) -> Result<Endpoint, std::io::Error> {
    let runtime = Arc::new(AsyncStdRuntime);
    let mut config = EndpointConfig::default();
    //Note that client mtu size shoud be smaller than server's
    config.max_udp_payload_size(1400).print_error("Should config quinn client max_size to 1400");
    let mut endpoint = Endpoint::new_with_abstract_socket(config, None, socket, runtime)?;
    endpoint.set_default_client_config(quinn_plaintext::client_config());
    Ok(endpoint)
}
