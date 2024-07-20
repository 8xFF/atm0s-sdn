use std::{collections::VecDeque, fmt::Debug, hash::Hash, net::SocketAddr, sync::Arc, time::Instant};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::{
    base::{Authorization, HandshakeBuilder, ServiceBuilder},
    controller_plane::ControllerPlaneCfg,
    data_plane::{DataPlaneCfg, NetInput, NetOutput},
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

enum State {
    Running,
    Shutdowning,
    Shutdowned,
}

pub struct SdnWorkerInner<UserData, SC, SE, TC, TW> {
    worker: u16,
    worker_inner: SdnWorker<UserData, SC, SE, TC, TW>,
    state: State,
    timer: TimePivot,
    #[cfg(feature = "vpn")]
    _vpn_tun_device: Option<sans_io_runtime::backend::tun::TunDevice>,
    udp_backend_slot: Option<usize>,
    #[cfg(feature = "vpn")]
    tun_backend_slot: Option<usize>,
    #[allow(clippy::type_complexity)]
    queue: VecDeque<WorkerInnerOutput<SdnOwner, SdnExtOut<UserData, SE>, SdnChannel, SdnEvent<UserData, SC, SE, TC, TW>, SdnSpawnCfg>>,
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
                    NetOutput::UdpPacket(dest, data) => BackendOutgoing::UdpPacket {
                        slot: self.udp_backend_slot.expect("Should have backend slot"),
                        to: dest,
                        data,
                    },
                    NetOutput::UdpPackets(dests, data) => BackendOutgoing::UdpPackets {
                        slot: self.udp_backend_slot.expect("Should have backend slot"),
                        to: dests,
                        data,
                    },
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
            SdnWorkerOutput::ShutdownResponse => {
                self.state = State::Shutdowned;
                Some(WorkerInnerOutput::Destroy(SdnOwner))
            }
            SdnWorkerOutput::Continue => {
                //we need to continue pop for continue gather output
                let out = self.worker_inner.pop_output2(now_ms)?;
                self.convert_output(now_ms, out)
            }
        }
    }
}

impl<UserData: 'static + Eq + Copy + Hash + Debug, SC: Debug, SE: Debug, TC: Debug, TW: Debug>
    WorkerInner<SdnOwner, SdnExtIn<UserData, SC>, SdnExtOut<UserData, SE>, SdnChannel, SdnEvent<UserData, SC, SE, TC, TW>, SdnInnerCfg<UserData, SC, SE, TC, TW>, SdnSpawnCfg>
    for SdnWorkerInner<UserData, SC, SE, TC, TW>
{
    fn build(worker: u16, cfg: SdnInnerCfg<UserData, SC, SE, TC, TW>) -> Self {
        // TODO implement multi bind_addrs;
        assert!(cfg.bind_addrs.len() == 1, "Current implementation only support single bind_addr");
        assert!(!cfg.bind_addrs[0].ip().is_unspecified(), "Current implementation only support non unspecified bind_addr");
        let addr = cfg.bind_addrs[0];
        let mut queue = VecDeque::from([
            WorkerInnerOutput::Bus(BusControl::Channel(SdnOwner, BusChannelControl::Subscribe(SdnChannel::Worker(worker)))),
            WorkerInnerOutput::Net(SdnOwner, BackendOutgoing::UdpListen { addr, reuse: true }),
        ]);
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
                state: State::Running,
                queue,
                udp_backend_slot: None,
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
                state: State::Running,
                queue,
                udp_backend_slot: None,
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

    fn spawn(&mut self, _now: Instant, _cfg: SdnSpawnCfg) {
        todo!("Spawn not implemented")
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
                    self.udp_backend_slot = Some(result.expect("Should have slot").1);
                }
                BackendIncoming::UdpPacket { slot: _, from, data } => self.worker_inner.on_event(now_ms, SdnWorkerInput::Net(NetInput::UdpPacket(from, data))),
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
        if !matches!(self.state, State::Running) {
            return;
        }

        let now_ms = self.timer.timestamp_ms(now);
        self.state = State::Shutdowning;
        self.worker_inner.on_event(now_ms, SdnWorkerInput::ShutdownRequest);
    }
}
