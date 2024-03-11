use std::net::Ipv4Addr;

use atm0s_sdn_identity::{NodeAddr, NodeAddrBuilder, NodeId, Protocol};
use sans_io_runtime::{backend::PollBackend, Owner};

use crate::tasks::{ControllerCfg, SdnController, SdnExtIn, SdnInnerCfg, SdnWorkerInner};

pub struct SdnBuilder {
    node_id: NodeId,
    udp_port: u16,
    seeds: Vec<NodeAddr>,
}

impl SdnBuilder {
    pub fn new(node_id: NodeId, udp_port: u16) -> Self {
        Self { node_id, udp_port, seeds: vec![] }
    }

    pub fn add_seed(&mut self, addr: NodeAddr) {
        self.seeds.push(addr);
    }

    pub fn build(self, workers: usize) -> SdnController {
        assert!(workers > 0);
        let mut addr_builder = NodeAddrBuilder::new(self.node_id);
        addr_builder.add_protocol(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)));
        addr_builder.add_protocol(Protocol::Udp(self.udp_port));
        let addr = addr_builder.addr();
        log::info!("Created node on addr {}", addr);

        let mut controler = SdnController::default();
        controler.add_worker::<_, SdnWorkerInner, PollBackend<128, 128>>(
            SdnInnerCfg {
                node_id: self.node_id,
                udp_port: self.udp_port,
                controller: Some(ControllerCfg {
                    password: "password".to_string(),
                    tick_ms: 1000,
                }),
            },
            None,
        );

        for _ in 1..workers {
            controler.add_worker::<_, SdnWorkerInner, PollBackend<128, 128>>(
                SdnInnerCfg {
                    node_id: self.node_id,
                    udp_port: self.udp_port,
                    controller: None,
                },
                None,
            );
        }

        std::thread::sleep(std::time::Duration::from_millis(100));

        for seed in self.seeds {
            controler.send_to(Owner::worker(0), SdnExtIn::ConnectTo(seed));
        }

        controler
    }
}
