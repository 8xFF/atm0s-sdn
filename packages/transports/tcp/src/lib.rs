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

    use atm0s_sdn_identity::{NodeAddr, NodeAddrBuilder, Protocol};

    use crate::TcpTransport;

    #[async_std::test]
    async fn simple_network() {
        let secure = Arc::new(atm0s_sdn_network::secure::StaticKeySecure::new("secure-token"));
        let mut node_addr_builder1 = NodeAddrBuilder::new(1);
        let listener1 = TcpTransport::prepare(0, &mut node_addr_builder1).await;
        let tran1 = TcpTransport::new(node_addr_builder1.addr(), listener1, secure.clone());

        let mut node_addr_builder2 = NodeAddrBuilder::new(2);
        let listener2 = TcpTransport::prepare(0, &mut node_addr_builder2).await;
        let tran2 = TcpTransport::new(node_addr_builder2.addr(), listener2, secure);

        atm0s_sdn_network::transport_tests::simple::simple_network(tran1, node_addr_builder1.addr(), tran2, node_addr_builder2.addr()).await;
    }

    #[async_std::test]
    async fn simple_network_connect_addr_not_found() {
        let secure = Arc::new(atm0s_sdn_network::secure::StaticKeySecure::new("secure-token"));
        let mut node_addr_builder1 = NodeAddrBuilder::new(1);
        let listener1 = TcpTransport::prepare(0, &mut node_addr_builder1).await;
        let tran1 = TcpTransport::new(node_addr_builder1.addr(), listener1, secure);
        atm0s_sdn_network::transport_tests::simple::simple_network_connect_addr_not_found(tran1, NodeAddr::from_iter(2, vec![Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)), Protocol::Tcp(20002)])).await;
    }

    #[async_std::test]
    async fn simple_network_connect_wrong_node() {
        let secure = Arc::new(atm0s_sdn_network::secure::StaticKeySecure::new("secure-token"));
        let mut node_addr_builder1 = NodeAddrBuilder::new(1);
        let listener1 = TcpTransport::prepare(0, &mut node_addr_builder1).await;
        let tran1 = TcpTransport::new(node_addr_builder1.addr(), listener1, secure.clone());

        let mut node_addr_builder2 = NodeAddrBuilder::new(2);
        let listener2 = TcpTransport::prepare(0, &mut node_addr_builder2).await;
        let tran2 = TcpTransport::new(node_addr_builder2.addr(), listener2, secure);

        let fake_node2_addr = NodeAddr::from_iter(3, node_addr_builder2.addr().multiaddr().iter());
        atm0s_sdn_network::transport_tests::simple::simple_network_connect_wrong_node(tran1, node_addr_builder1.addr(), tran2, fake_node2_addr).await;
    }
}
