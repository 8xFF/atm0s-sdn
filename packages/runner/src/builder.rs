use std::{
    fmt::Debug,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use atm0s_sdn_identity::{NodeAddr, NodeAddrBuilder, NodeId, Protocol};
use atm0s_sdn_network::{
    base::ServiceBuilder,
    features::{FeaturesControl, FeaturesEvent},
    services::manual_discovery,
};
use atm0s_sdn_utils::random::{self, Random};
use sans_io_runtime::{backend::Backend, Owner};

use crate::tasks::{ControllerCfg, SdnController, SdnExtIn, SdnInnerCfg, SdnWorkerInner};

pub struct SdnBuilder<TC, TW> {
    node_addr: NodeAddr,
    node_id: NodeId,
    session: u64,
    udp_port: u16,
    seeds: Vec<NodeAddr>,
    services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, TC, TW>>>,
    #[cfg(feature = "vpn")]
    vpn_ip: Option<(u8, u8, u8, u8)>,
    #[cfg(feature = "vpn")]
    vpn_netmask: Option<(u8, u8, u8, u8)>,
}

impl<TC: Debug, TW: Debug> SdnBuilder<TC, TW>
where
    TC: 'static + Clone + Send + Sync,
    TW: 'static + Clone + Send + Sync,
{
    pub fn new(node_id: NodeId, udp_port: u16, custom_ip: Vec<SocketAddr>) -> Self {
        let mut addr_builder = NodeAddrBuilder::new(node_id);
        for (name, ip) in local_ip_address::list_afinet_netifas().expect("Should have local ip addresses") {
            match ip {
                IpAddr::V4(ip) => {
                    log::info!("Added ip {}:\t{:?}", name, ip);
                    addr_builder.add_protocol(Protocol::Ip4(ip));
                    addr_builder.add_protocol(Protocol::Udp(udp_port));
                }
                IpAddr::V6(ip) => {
                    log::warn!("Ignoring ipv6 address: {}", ip);
                }
            }
        }
        for ip in custom_ip {
            match ip {
                SocketAddr::V4(ip) => {
                    log::info!("Added custom ip:\t{:?}", ip);
                    addr_builder.add_protocol(Protocol::Ip4(*ip.ip()));
                    addr_builder.add_protocol(Protocol::Udp(ip.port()));
                }
                SocketAddr::V6(ip) => {
                    log::warn!("Ignoring ipv6 address: {}", ip);
                }
            }
        }

        let node_addr = addr_builder.addr();
        log::info!("Created node on addr {}", node_addr);

        Self {
            node_addr,
            node_id,
            session: random::RealRandom().random(),
            udp_port,
            seeds: vec![],
            services: vec![],
            // services: vec![],
            #[cfg(feature = "vpn")]
            vpn_ip: None,
            #[cfg(feature = "vpn")]
            vpn_netmask: None,
        }
    }

    pub fn node_addr(&self) -> NodeAddr {
        self.node_addr.clone()
    }

    pub fn add_seed(&mut self, addr: NodeAddr) {
        self.seeds.push(addr);
    }

    /// Setting manual discovery
    pub fn set_manual_discovery(&mut self, local_tags: Vec<String>, connect_tags: Vec<String>) {
        self.add_service(Arc::new(manual_discovery::ManualDiscoveryServiceBuilder::new(self.node_addr.clone(), local_tags, connect_tags)));
    }

    /// panic if the service already exists
    pub fn add_service(&mut self, service: Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, TC, TW>>) {
        for s in self.services.iter() {
            assert_ne!(s.service_id(), service.service_id(), "Service ({}, {}) already exists", service.service_id(), service.service_name());
        }
        self.services.push(service);
    }

    #[cfg(feature = "vpn")]
    pub fn set_vpn_ip(&mut self, ip: (u8, u8, u8, u8)) {
        self.vpn_ip = Some(ip);
    }

    #[cfg(feature = "vpn")]
    pub fn set_vpn_netmask(&mut self, netmask: (u8, u8, u8, u8)) {
        self.vpn_netmask = Some(netmask);
    }

    pub fn build<B: Backend>(self, workers: usize) -> SdnController<TC, TW> {
        assert!(workers > 0);
        #[cfg(feature = "vpn")]
        let (tun_device, mut queue_fds) = {
            let vpn_ip = self.vpn_ip.unwrap_or((10, 33, 33, self.node_id as u8));
            let vpn_netmask = self.vpn_netmask.unwrap_or((255, 255, 255, 0));
            let mut tun_device = sans_io_runtime::backend::tun::create_tun(&format!("utun{}", self.node_id as u8), vpn_ip, vpn_netmask, 1400, workers);
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
                services: self.services.clone(),
                controller: Some(ControllerCfg {
                    session: self.session,
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
                    services: self.services.clone(),
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
