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
    use std::net::Ipv4Addr;

    use crate::transport::UdpTransport;
    use atm0s_sdn_identity::{NodeAddr, NodeAddrBuilder, Protocol};

    #[async_std::test]
    async fn simple_network() {
        let mut node_addr_builder1 = NodeAddrBuilder::new(1);
        let sock1 = UdpTransport::prepare(0, &mut node_addr_builder1).await;
        let tran1 = UdpTransport::new(node_addr_builder1.addr(), sock1);

        let mut node_addr_builder2 = NodeAddrBuilder::new(2);
        let sock2 = UdpTransport::prepare(0, &mut node_addr_builder2).await;
        let tran2 = UdpTransport::new(node_addr_builder2.addr(), sock2);

        atm0s_sdn_network::transport_tests::simple::simple_network(tran1, node_addr_builder1.addr(), tran2, node_addr_builder2.addr()).await;
    }

    #[async_std::test]
    async fn simple_network_connect_addr_not_found() {
        let mut node_addr_builder1 = NodeAddrBuilder::new(1);
        let sock1 = UdpTransport::prepare(0, &mut node_addr_builder1).await;
        let tran1 = UdpTransport::new(node_addr_builder1.addr(), sock1);
        atm0s_sdn_network::transport_tests::simple::simple_network_connect_addr_not_found(tran1, NodeAddr::from_iter(2, vec![Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)), Protocol::Udp(20002)])).await;
    }

    #[async_std::test]
    async fn simple_network_connect_wrong_node() {
        let mut node_addr_builder1 = NodeAddrBuilder::new(1);
        let sock1 = UdpTransport::prepare(0, &mut node_addr_builder1).await;
        let tran1 = UdpTransport::new(node_addr_builder1.addr(), sock1);

        let mut node_addr_builder2 = NodeAddrBuilder::new(2);
        let sock2 = UdpTransport::prepare(0, &mut node_addr_builder2).await;
        let tran2 = UdpTransport::new(node_addr_builder2.addr(), sock2);

        let fake_node2_addr = NodeAddr::from_iter(3, node_addr_builder2.addr().multiaddr().iter());
        atm0s_sdn_network::transport_tests::simple::simple_network_connect_wrong_node(tran1, node_addr_builder1.addr(), tran2, fake_node2_addr).await;
    }
}
