mod connection;
mod connector;
mod handshake;
mod msg;
mod transport;

pub const TCP_PROTOCOL_ID: u8 = 2;
pub use transport::TcpTransport;

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, sync::Arc};

    use bluesea_identity::{NodeAddr, NodeAddrBuilder, Protocol};

    use crate::TcpTransport;

    #[async_std::test]
    async fn simple_network() {
        let node_addr_builder1 = Arc::new(NodeAddrBuilder::default());
        let tran1 = TcpTransport::new(1, 10001, node_addr_builder1.clone()).await;

        let node_addr_builder2 = Arc::new(NodeAddrBuilder::default());
        let tran2 = TcpTransport::new(2, 10002, node_addr_builder2.clone()).await;

        network::transport_tests::simple::simple_network(tran1, node_addr_builder1.addr(), tran2, node_addr_builder2.addr()).await;
    }

    #[async_std::test]
    async fn simple_network_connect_addr_not_found() {
        let node_addr_builder1 = Arc::new(NodeAddrBuilder::default());
        let tran1 = TcpTransport::new(1, 20001, node_addr_builder1.clone()).await;
        network::transport_tests::simple::simple_network_connect_addr_not_found(tran1, NodeAddr::from_iter(vec![Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)), Protocol::Tcp(20002)])).await;
    }

    #[async_std::test]
    async fn simple_network_connect_wrong_node() {
        let node_addr_builder1 = Arc::new(NodeAddrBuilder::default());
        let tran1 = TcpTransport::new(1, 30001, node_addr_builder1.clone()).await;

        let node_addr_builder2 = Arc::new(NodeAddrBuilder::default());
        let tran2 = TcpTransport::new(2, 30002, node_addr_builder2.clone()).await;

        network::transport_tests::simple::simple_network_connect_wrong_node(tran1, node_addr_builder1.addr(), tran2, node_addr_builder2.addr()).await;
    }
}
