use std::{
    fmt::Debug,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use atm0s_sdn_identity::{NodeAddr, NodeAddrBuilder, NodeId, Protocol};
use atm0s_sdn_network::{
    base::{Authorization, ServiceBuilder},
    features::{FeaturesControl, FeaturesEvent},
    services::{manual_discovery, visualization},
};
use rand::{thread_rng, RngCore};
use sans_io_runtime::{backend::Backend, Owner};

use crate::tasks::{ControllerCfg, DataWorkerHistory, SdnController, SdnExtIn, SdnInnerCfg, SdnWorkerInner};

pub struct SdnBuilder<SC, SE, TC, TW> {
    auth: Arc<dyn Authorization + Send + Sync>,
    node_addr: NodeAddr,
    node_id: NodeId,
    session: u64,
    udp_port: u16,
    visualization_collector: bool,
    seeds: Vec<NodeAddr>,
    services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, SC, SE, TC, TW>>>,
    #[cfg(feature = "vpn")]
    vpn_enable: bool,
    #[cfg(feature = "vpn")]
    vpn_ip: Option<(u8, u8, u8, u8)>,
    #[cfg(feature = "vpn")]
    vpn_netmask: Option<(u8, u8, u8, u8)>,
}

impl<SC, SE, TC: Debug, TW: Debug> SdnBuilder<SC, SE, TC, TW>
where
    SC: 'static + Clone + Debug + Send + Sync + From<visualization::Control> + TryInto<visualization::Control>,
    SE: 'static + Clone + Debug + Send + Sync + From<visualization::Event> + TryInto<visualization::Event>,
    TC: 'static + Clone + Send + Sync,
    TW: 'static + Clone + Send + Sync,
{
    pub fn new(node_id: NodeId, udp_port: u16, custom_ip: Vec<SocketAddr>, auth: Arc<dyn Authorization + Send + Sync>) -> Self {
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
            auth,
            node_addr,
            node_id,
            session: thread_rng().next_u64(),
            udp_port,
            visualization_collector: false,
            seeds: vec![],
            services: vec![],
            #[cfg(feature = "vpn")]
            vpn_enable: false,
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

    /// Setting visualization collector mode
    pub fn set_visualization_collector(&mut self, value: bool) {
        self.visualization_collector = value;
    }

    /// Setting manual discovery
    pub fn set_manual_discovery(&mut self, local_tags: Vec<String>, connect_tags: Vec<String>) {
        self.add_service(Arc::new(manual_discovery::ManualDiscoveryServiceBuilder::new(self.node_addr.clone(), local_tags, connect_tags)));
    }

    /// panic if the service already exists
    pub fn add_service(&mut self, service: Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, SC, SE, TC, TW>>) {
        for s in self.services.iter() {
            assert_ne!(s.service_id(), service.service_id(), "Service ({}, {}) already exists", service.service_id(), service.service_name());
        }
        self.services.push(service);
    }

    #[cfg(feature = "vpn")]
    pub fn enable_vpn(&mut self) {
        self.vpn_enable = true;
    }

    #[cfg(feature = "vpn")]
    pub fn set_vpn_ip(&mut self, ip: (u8, u8, u8, u8)) {
        self.vpn_ip = Some(ip);
    }

    #[cfg(feature = "vpn")]
    pub fn set_vpn_netmask(&mut self, netmask: (u8, u8, u8, u8)) {
        self.vpn_netmask = Some(netmask);
    }

    pub fn build<B: Backend>(mut self, workers: usize) -> SdnController<SC, SE, TC, TW> {
        assert!(workers > 0);
        #[cfg(feature = "vpn")]
        let (tun_device, mut queue_fds) = {
            if self.vpn_enable {
                let vpn_ip = self.vpn_ip.unwrap_or((10, 33, 33, self.node_id as u8));
                let vpn_netmask = self.vpn_netmask.unwrap_or((255, 255, 255, 0));
                let mut tun_device = sans_io_runtime::backend::tun::create_tun(&format!("utun{}", self.node_id as u8), vpn_ip, vpn_netmask, 1400, workers);
                let mut queue_fds = std::collections::VecDeque::with_capacity(workers);
                for i in 0..workers {
                    queue_fds.push_back(tun_device.get_queue_fd(i).expect("Should have tun queue fd"));
                }
                (Some(tun_device), queue_fds)
            } else {
                (None, std::collections::VecDeque::new())
            }
        };

        self.add_service(Arc::new(visualization::VisualizationServiceBuilder::<SC, SE, TC, TW>::new(self.visualization_collector)));

        let history = Arc::new(DataWorkerHistory::default());

        let mut controller = SdnController::default();
        controller.add_worker::<_, SdnWorkerInner<SC, SE, TC, TW>, B>(
            SdnInnerCfg {
                node_id: self.node_id,
                udp_port: self.udp_port,
                services: self.services.clone(),
                history: history.clone(),
                controller: Some(ControllerCfg {
                    session: self.session,
                    auth: self.auth,
                    tick_ms: 1000,
                    #[cfg(feature = "vpn")]
                    vpn_tun_device: tun_device,
                }),
                #[cfg(feature = "vpn")]
                vpn_tun_fd: queue_fds.pop_front(),
            },
            None,
        );

        for _ in 1..workers {
            controller.add_worker::<_, SdnWorkerInner<SC, SE, TC, TW>, B>(
                SdnInnerCfg {
                    node_id: self.node_id,
                    udp_port: self.udp_port,
                    services: self.services.clone(),
                    history: history.clone(),
                    controller: None,
                    #[cfg(feature = "vpn")]
                    vpn_tun_fd: queue_fds.pop_front(),
                },
                None,
            );
        }

        std::thread::sleep(std::time::Duration::from_millis(100));

        for seed in self.seeds {
            controller.send_to(Owner::worker(0), SdnExtIn::ConnectTo(seed));
        }

        controller
    }
}
