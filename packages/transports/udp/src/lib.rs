mod connector;
mod handshake;
mod msg;
mod receiver;
mod sender;
mod transport;

pub const UDP_PROTOCOL_ID: u8 = 3;
pub use transport::UdpTransport;

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, sync::Arc};

    use crate::transport::UdpTransport;
    use atm0s_sdn_identity::{NodeAddr, NodeAddrBuilder, Protocol};

    #[async_std::test]
    async fn simple_network() {
        let node_addr_builder1 = Arc::new(NodeAddrBuilder::default());
        let tran1 = UdpTransport::new(1, 0, node_addr_builder1.clone()).await;

        let node_addr_builder2 = Arc::new(NodeAddrBuilder::default());
        let tran2 = UdpTransport::new(2, 0, node_addr_builder2.clone()).await;

        atm0s_sdn_network::transport_tests::simple::simple_network(tran1, node_addr_builder1.addr(), tran2, node_addr_builder2.addr()).await;
    }

    #[async_std::test]
    async fn simple_network_connect_addr_not_found() {
        let node_addr_builder1 = Arc::new(NodeAddrBuilder::default());
        let tran1 = UdpTransport::new(1, 0, node_addr_builder1.clone()).await;
        atm0s_sdn_network::transport_tests::simple::simple_network_connect_addr_not_found(tran1, NodeAddr::from_iter(vec![Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)), Protocol::Udp(20002)])).await;
    }

    #[async_std::test]
    async fn simple_network_connect_wrong_node() {
        let node_addr_builder1 = Arc::new(NodeAddrBuilder::default());
        let tran1 = UdpTransport::new(1, 0, node_addr_builder1.clone()).await;

        let node_addr_builder2 = Arc::new(NodeAddrBuilder::default());
        let tran2 = UdpTransport::new(2, 0, node_addr_builder2.clone()).await;

        atm0s_sdn_network::transport_tests::simple::simple_network_connect_wrong_node(tran1, node_addr_builder1.addr(), tran2, node_addr_builder2.addr()).await;
    }
}
