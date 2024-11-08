use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
    hash::Hash,
    net::SocketAddr,
    sync::Arc,
    time::Instant,
};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::{
    base::{Authorization, HandshakeBuilder, ServiceBuilder},
    controller_plane::ControllerPlaneCfg,
    data_plane::{DataPlaneCfg, NetInput, NetOutput, NetPair},
    features::{FeaturesControl, FeaturesEvent},
    worker::{SdnWorker, SdnWorkerBusEvent, SdnWorkerCfg, SdnWorkerInput, SdnWorkerOutput},
    ExtIn, ExtOut,
};
use atm0s_sdn_router::shadow::ShadowRouterHistory;
use rand::rngs::OsRng;
use sans_io_runtime::{
    backend::{BackendIncoming, BackendOutgoing},
    BusChannelControl, BusControl, BusEvent, Controller, WorkerInner, WorkerInnerInput, WorkerInnerOutput,
};

use crate::time::TimePivot;

pub type SdnController<UserData, SC, SE, TC, TW> = Controller<SdnExtIn<UserData, SC>, SdnExtOut<UserData, SE>, SdnSpawnCfg, SdnChannel, SdnEvent<UserData, SC, SE, TC, TW>, 1024>;

pub type SdnExtIn<UserData, SC> = ExtIn<UserData, SC>;
pub type SdnExtOut<UserData, SE> = ExtOut<UserData, SE>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SdnOwner;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SdnChannel {
    Controller,
    Worker(u16),
}

pub type SdnEvent<UserData, SC, SE, TC, TW> = SdnWorkerBusEvent<UserData, SC, SE, TC, TW>;

pub struct ControllerCfg {
    pub session: u64,
    pub auth: Arc<dyn Authorization>,
    pub handshake: Arc<dyn HandshakeBuilder>,
    #[cfg(feature = "vpn")]
    pub vpn_tun_device: Option<sans_io_runtime::backend::tun::TunDevice>,
}

pub struct SdnInnerCfg<UserData, SC, SE, TC, TW> {
    pub node_id: NodeId,
    pub tick_ms: u64,
    pub bind_addrs: Vec<SocketAddr>,
    pub controller: Option<ControllerCfg>,
    #[allow(clippy::type_complexity)]
    pub services: Vec<Arc<dyn ServiceBuilder<UserData, FeaturesControl, FeaturesEvent, SC, SE, TC, TW>>>,
    pub history: Arc<dyn ShadowRouterHistory>,
    #[cfg(feature = "vpn")]
    pub vpn_tun_fd: Option<sans_io_runtime::backend::tun::TunFd>,
}

pub type SdnSpawnCfg = ();

pub struct SdnWorkerInner<UserData, SC, SE, TC, TW> {
    worker: u16,
    worker_inner: SdnWorker<UserData, SC, SE, TC, TW>,
    timer: TimePivot,
    #[cfg(feature = "vpn")]
    _vpn_tun_device: Option<sans_io_runtime::backend::tun::TunDevice>,
    bind_addrs: HashMap<SocketAddr, usize>,
    bind_slots: HashMap<usize, SocketAddr>,
    #[cfg(feature = "vpn")]
    tun_backend_slot: Option<usize>,
    #[allow(clippy::type_complexity)]
    queue: VecDeque<WorkerInnerOutput<SdnOwner, SdnExtOut<UserData, SE>, SdnChannel, SdnEvent<UserData, SC, SE, TC, TW>, SdnSpawnCfg>>,
    shutdown: bool,
}

#[allow(clippy::type_complexity)]
impl<UserData: 'static + Eq + Copy + Hash + Debug, SC: Debug, SE: Debug, TC: Debug, TW: Debug> SdnWorkerInner<UserData, SC, SE, TC, TW> {
    fn convert_output(
        &mut self,
        now_ms: u64,
        event: SdnWorkerOutput<UserData, SC, SE, TC, TW>,
    ) -> Option<WorkerInnerOutput<SdnOwner, SdnExtOut<UserData, SE>, SdnChannel, SdnEvent<UserData, SC, SE, TC, TW>, SdnSpawnCfg>> {
        match event {
            SdnWorkerOutput::Ext(ext) => Some(WorkerInnerOutput::Ext(true, ext)),
            SdnWorkerOutput::ExtWorker(_) => {
                panic!("should not have ExtWorker with standalone node")
            }
            SdnWorkerOutput::Net(net) => {
                let out = match net {
                    NetOutput::UdpPacket(pair, data) => BackendOutgoing::UdpPacket {
                        slot: *self.bind_addrs.get(&pair.local)?,
                        to: pair.remote,
                        data,
                    },
                    NetOutput::UdpPackets(pairs, data) => {
                        let to = pairs.into_iter().filter_map(|p| self.bind_addrs.get(&p.local).map(|s| (*s, p.remote))).collect::<Vec<_>>();
                        BackendOutgoing::UdpPackets2 { to, data }
                    }
                    #[cfg(feature = "vpn")]
                    NetOutput::TunPacket(data) => BackendOutgoing::TunPacket {
                        slot: self.tun_backend_slot.expect("should have tun"),
                        data,
                    },
                };
                Some(WorkerInnerOutput::Net(SdnOwner, out))
            }
            SdnWorkerOutput::Bus(event) => match &event {
                SdnWorkerBusEvent::Control(..) => Some(WorkerInnerOutput::Bus(BusControl::Channel(SdnOwner, BusChannelControl::Publish(SdnChannel::Controller, true, event)))),
                SdnWorkerBusEvent::Workers(..) => Some(WorkerInnerOutput::Bus(BusControl::Broadcast(true, event))),
                SdnWorkerBusEvent::Worker(worker, _msg) => Some(WorkerInnerOutput::Bus(BusControl::Channel(
                    SdnOwner,
                    BusChannelControl::Publish(SdnChannel::Worker(*worker), true, event),
                ))),
            },
            SdnWorkerOutput::Continue => {
                //we need to continue pop for continue gather output
                let out = self.worker_inner.pop_output2(now_ms)?;
                self.convert_output(now_ms, out)
            }
            SdnWorkerOutput::OnResourceEmpty => Some(WorkerInnerOutput::Continue),
        }
    }
}

impl<UserData: 'static + Eq + Copy + Hash + Debug, SC: Debug, SE: Debug, TC: Debug, TW: Debug>
    WorkerInner<SdnOwner, SdnExtIn<UserData, SC>, SdnExtOut<UserData, SE>, SdnChannel, SdnEvent<UserData, SC, SE, TC, TW>, SdnInnerCfg<UserData, SC, SE, TC, TW>, SdnSpawnCfg>
    for SdnWorkerInner<UserData, SC, SE, TC, TW>
{
    fn build(worker: u16, cfg: SdnInnerCfg<UserData, SC, SE, TC, TW>) -> Self {
        let mut queue = VecDeque::from([WorkerInnerOutput::Bus(BusControl::Channel(SdnOwner, BusChannelControl::Subscribe(SdnChannel::Worker(worker))))]);

        for addr in &cfg.bind_addrs {
            queue.push_back(WorkerInnerOutput::Net(SdnOwner, BackendOutgoing::UdpListen { addr: *addr, reuse: true }));
        }

        #[cfg(feature = "vpn")]
        if let Some(fd) = cfg.vpn_tun_fd {
            queue.push_back(WorkerInnerOutput::Net(SdnOwner, BackendOutgoing::TunBind { fd }));
        }
        if let Some(controller) = cfg.controller {
            queue.push_back(WorkerInnerOutput::Bus(BusControl::Channel(SdnOwner, BusChannelControl::Subscribe(SdnChannel::Controller))));
            log::info!("Create controller worker");
            Self {
                worker,
                worker_inner: SdnWorker::new(SdnWorkerCfg {
                    node_id: cfg.node_id,
                    tick_ms: cfg.tick_ms,
                    controller: Some(ControllerPlaneCfg {
                        bind_addrs: cfg.bind_addrs,
                        authorization: controller.auth,
                        handshake_builder: controller.handshake,
                        session: controller.session,
                        random: Box::new(OsRng),
                        services: cfg.services.clone(),
                        history: cfg.history.clone(),
                    }),
                    data: DataPlaneCfg {
                        worker_id: worker,
                        services: cfg.services,
                        history: cfg.history,
                    },
                }),
                timer: TimePivot::build(),
                #[cfg(feature = "vpn")]
                _vpn_tun_device: controller.vpn_tun_device,
                queue,
                shutdown: false,
                bind_addrs: Default::default(),
                bind_slots: Default::default(),
                #[cfg(feature = "vpn")]
                tun_backend_slot: None,
            }
        } else {
            log::info!("Create data only worker");
            Self {
                worker,
                worker_inner: SdnWorker::new(SdnWorkerCfg {
                    node_id: cfg.node_id,
                    tick_ms: cfg.tick_ms,
                    controller: None,
                    data: DataPlaneCfg {
                        worker_id: worker,
                        services: cfg.services,
                        history: cfg.history,
                    },
                }),
                timer: TimePivot::build(),
                #[cfg(feature = "vpn")]
                _vpn_tun_device: None,
                queue,
                shutdown: false,
                bind_addrs: Default::default(),
                bind_slots: Default::default(),
                #[cfg(feature = "vpn")]
                tun_backend_slot: None,
            }
        }
    }

    fn worker_index(&self) -> u16 {
        self.worker
    }

    fn tasks(&self) -> usize {
        self.worker_inner.tasks()
    }

    fn is_empty(&self) -> bool {
        self.shutdown && self.queue.is_empty() && self.worker_inner.is_empty()
    }

    fn spawn(&mut self, _now: Instant, _cfg: SdnSpawnCfg) {
        panic!("Spawn not supported")
    }

    fn on_tick(&mut self, now: Instant) {
        let now_ms = self.timer.timestamp_ms(now);
        self.worker_inner.on_tick(now_ms);
    }

    fn on_event(&mut self, now: Instant, event: WorkerInnerInput<SdnOwner, SdnExtIn<UserData, SC>, SdnChannel, SdnEvent<UserData, SC, SE, TC, TW>>) {
        let now_ms = self.timer.timestamp_ms(now);
        match event {
            WorkerInnerInput::Net(_, event) => match event {
                BackendIncoming::UdpListenResult { bind: _, result } => {
                    if let Ok((addr, slot)) = result {
                        log::info!("Worker {} bind addr {addr} to slot {slot}", self.worker);
                        self.bind_addrs.insert(addr, slot);
                        self.bind_slots.insert(slot, addr);
                    }
                }
                BackendIncoming::UdpPacket { slot, from, data } => {
                    let local = *self.bind_slots.get(&slot).expect("Should have local addr");
                    let pair = NetPair::new(local, from);
                    self.worker_inner.on_event(now_ms, SdnWorkerInput::Net(NetInput::UdpPacket(pair, data)))
                }
                #[cfg(feature = "vpn")]
                BackendIncoming::TunBindResult { result } => {
                    self.tun_backend_slot = Some(result.expect("Should have slot"));
                }
                #[cfg(feature = "vpn")]
                BackendIncoming::TunPacket { slot: _, data } => self.worker_inner.on_event(now_ms, SdnWorkerInput::Net(NetInput::TunPacket(data))),
            },
            WorkerInnerInput::Bus(event) => match event {
                BusEvent::Broadcast(_from_worker, msg) => self.worker_inner.on_event(now_ms, SdnWorkerInput::Bus(msg)),
                BusEvent::Channel(_, _, msg) => self.worker_inner.on_event(now_ms, SdnWorkerInput::Bus(msg)),
            },
            WorkerInnerInput::Ext(ext) => self.worker_inner.on_event(now_ms, SdnWorkerInput::Ext(ext)),
        };
    }

    fn pop_output(&mut self, now: Instant) -> Option<WorkerInnerOutput<SdnOwner, SdnExtOut<UserData, SE>, SdnChannel, SdnEvent<UserData, SC, SE, TC, TW>, SdnSpawnCfg>> {
        if let Some(e) = self.queue.pop_front() {
            return Some(e);
        }
        let now_ms = self.timer.timestamp_ms(now);
        let out = self.worker_inner.pop_output2(now_ms)?;
        self.convert_output(now_ms, out)
    }

    fn on_shutdown(&mut self, now: Instant) {
        if self.shutdown {
            return;
        }
        let now_ms = self.timer.timestamp_ms(now);
        self.worker_inner.on_shutdown(now_ms);
        for slot in self.bind_addrs.values() {
            self.queue.push_back(WorkerInnerOutput::Net(SdnOwner, BackendOutgoing::UdpUnlisten { slot: *slot }));
        }
        self.shutdown = true;
    }
}
