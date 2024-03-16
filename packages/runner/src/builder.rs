use std::{fmt::Debug, net::Ipv4Addr};

use atm0s_sdn_identity::{NodeAddr, NodeAddrBuilder, NodeId, Protocol};
use sans_io_runtime::{backend::Backend, Owner};

use crate::tasks::{ControllerCfg, SdnController, SdnExtIn, SdnInnerCfg, SdnWorkerInner};

pub struct SdnBuilder {
    node_id: NodeId,
    udp_port: u16,
    seeds: Vec<NodeAddr>,
    // services: Vec<Box<dyn atm0s_sdn_network::controller_plane::Service>>,
    #[cfg(feature = "vpn")]
    vpn_ip: Option<(u8, u8, u8, u8)>,
    #[cfg(feature = "vpn")]
    vpn_netmask: Option<(u8, u8, u8, u8)>,
}

impl SdnBuilder {
    pub fn new(node_id: NodeId, udp_port: u16) -> Self {
        Self {
            node_id,
            udp_port,
            seeds: vec![],
            // services: vec![],
            #[cfg(feature = "vpn")]
            vpn_ip: None,
            #[cfg(feature = "vpn")]
            vpn_netmask: None,
        }
    }

    pub fn add_seed(&mut self, addr: NodeAddr) {
        self.seeds.push(addr);
    }

    // pub fn add_service(&mut self, service: Box<dyn atm0s_sdn_network::controller_plane::Service>) {
    //     self.services.push(service);
    // }

    #[cfg(feature = "vpn")]
    pub fn set_vpn_ip(&mut self, ip: (u8, u8, u8, u8)) {
        self.vpn_ip = Some(ip);
    }

    #[cfg(feature = "vpn")]
    pub fn set_vpn_netmask(&mut self, netmask: (u8, u8, u8, u8)) {
        self.vpn_netmask = Some(netmask);
    }

    pub fn build<B: Backend, TC: Debug, TW: Debug>(self, workers: usize) -> SdnController<TC, TW>
    where
        TC: 'static + Clone + Send + Sync,
        TW: 'static + Clone + Send + Sync,
    {
        assert!(workers > 0);
        let mut addr_builder = NodeAddrBuilder::new(self.node_id);
        addr_builder.add_protocol(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)));
        addr_builder.add_protocol(Protocol::Udp(self.udp_port));
        let addr = addr_builder.addr();
        log::info!("Created node on addr {}", addr);

        #[cfg(feature = "vpn")]
        let (tun_device, mut queue_fds) = {
            let vpn_ip = self.vpn_ip.unwrap_or((10, 33, 33, self.node_id as u8));
            let vpn_netmask = self.vpn_netmask.unwrap_or((255, 255, 255, 0));
            let mut tun_device = sans_io_runtime::backend::tun::create_tun(&format!("utun{}", self.node_id as u8), vpn_ip, vpn_netmask, workers);
            let mut queue_fds = std::collections::VecDeque::with_capacity(workers);
            for i in 0..workers {
                queue_fds.push_back(tun_device.get_queue_fd(i).expect("Should have tun queue fd"));
            }
            (tun_device, queue_fds)
        };

        let mut controler = SdnController::default();
        controler.add_worker::<_, SdnWorkerInner<TC, TW>, B>(
            SdnInnerCfg {
                node_id: self.node_id,
                udp_port: self.udp_port,
                controller: Some(ControllerCfg {
                    password: "password".to_string(),
                    tick_ms: 1000,
                    // services: self.services,
                    #[cfg(feature = "vpn")]
                    vpn_tun_device: tun_device,
                }),
                #[cfg(feature = "vpn")]
                vpn_tun_fd: queue_fds.pop_front().expect("Should have tun queue fd"),
            },
            None,
        );

        for _ in 1..workers {
            controler.add_worker::<_, SdnWorkerInner<TC, TW>, B>(
                SdnInnerCfg {
                    node_id: self.node_id,
                    udp_port: self.udp_port,
                    controller: None,
                    #[cfg(feature = "vpn")]
                    vpn_tun_fd: queue_fds.pop_front().expect("Should have tun queue fd"),
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
