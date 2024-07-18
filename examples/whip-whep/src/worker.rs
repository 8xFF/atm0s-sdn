use std::{collections::VecDeque, net::SocketAddr, sync::Arc, time::Instant};

use atm0s_sdn::sans_io_runtime::{
    backend::{BackendIncoming, BackendOutgoing},
    return_if_some, BusChannelControl, BusControl, BusEvent, TaskSwitcher, WorkerInner, WorkerInnerInput, WorkerInnerOutput,
};
use atm0s_sdn::{
    base::{Authorization, HandshakeBuilder, ServiceBuilder},
    features::{FeaturesControl, FeaturesEvent},
    services::visualization,
    ControllerPlaneCfg, DataPlaneCfg, NetInput, NetOutput, NodeId, SdnChannel, SdnEvent, SdnExtIn, SdnExtOut, SdnOwner, SdnWorker, SdnWorkerBusEvent, SdnWorkerCfg, SdnWorkerInput, SdnWorkerOutput,
    ShadowRouterHistory, TimePivot,
};
use rand::rngs::OsRng;

use crate::{
    http::{HttpRequest, HttpResponse},
    sfu::{self, SfuChannel, SfuOwner, SfuWorker},
};

#[repr(usize)]
enum TaskType {
    Sfu = 0,
    Sdn = 1,
}

impl TryFrom<usize> for TaskType {
    type Error = ();
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Sfu),
            1 => Ok(Self::Sdn),
            _ => Err(()),
        }
    }
}

#[derive(convert_enum::From, convert_enum::TryInto, Clone, Debug)]
pub enum ExtIn {
    Sdn(SdnExtIn<(), SC>),
    HttpRequest(HttpRequest),
}

#[derive(convert_enum::From, convert_enum::TryInto, Clone)]
pub enum ExtOut {
    Sdn(SdnExtOut<(), SE>),
    HttpResponse(HttpResponse),
}

#[derive(convert_enum::From, convert_enum::TryInto, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelId {
    Sfu(SfuChannel),
    Sdn(SdnChannel),
}

#[derive(convert_enum::From, convert_enum::TryInto, Clone, Debug)]
pub enum Event {
    Sdn(SdnEvent<(), SC, SE, TC, TW>),
}

pub struct ControllerCfg {
    pub session: u64,
    pub auth: Arc<dyn Authorization>,
    pub handshake: Arc<dyn HandshakeBuilder>,
    #[cfg(feature = "vpn")]
    pub vpn_tun_device: Option<sans_io_runtime::backend::tun::TunDevice>,
}

pub struct SdnInnerCfg<SC, SE, TC, TW> {
    pub node_id: NodeId,
    pub tick_ms: u64,
    pub controller: Option<ControllerCfg>,
    pub services: Vec<Arc<dyn ServiceBuilder<(), FeaturesControl, FeaturesEvent, SC, SE, TC, TW>>>,
    pub history: Arc<dyn ShadowRouterHistory>,
    #[cfg(feature = "vpn")]
    pub vpn_tun_fd: Option<sans_io_runtime::backend::tun::TunFd>,
}

pub struct ICfg {
    pub sdn: SdnInnerCfg<SC, SE, TC, TW>,
    pub sdn_listen: SocketAddr,
    pub sfu: SocketAddr,
}

#[derive(convert_enum::From, convert_enum::TryInto)]
pub enum SCfg {}

pub type SC = visualization::Control;
pub type SE = visualization::Event;
pub type TC = ();
pub type TW = ();

#[derive(convert_enum::From, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunnerOwner {
    Sdn(SdnOwner),
    Sfu(SfuOwner),
}

pub struct RunnerWorker {
    worker: u16,
    sdn: SdnWorker<(), SC, SE, TC, TW>,
    sfu: SfuWorker,
    sdn_backend_slot: usize,
    sfu_backend_slot: usize,
    switcher: TaskSwitcher,
    time: TimePivot,
    queue: VecDeque<WorkerInnerOutput<RunnerOwner, ExtOut, ChannelId, Event, SCfg>>,
}

impl WorkerInner<RunnerOwner, ExtIn, ExtOut, ChannelId, Event, ICfg, SCfg> for RunnerWorker {
    fn build(worker: u16, cfg: ICfg) -> Self {
        let mut queue = VecDeque::from([
            WorkerInnerOutput::Net(RunnerOwner::Sdn(SdnOwner), BackendOutgoing::UdpListen { addr: cfg.sdn_listen, reuse: true }),
            WorkerInnerOutput::Net(RunnerOwner::Sfu(SfuOwner), BackendOutgoing::UdpListen { addr: cfg.sfu, reuse: false }),
            WorkerInnerOutput::Bus(BusControl::Channel(
                RunnerOwner::Sdn(SdnOwner),
                BusChannelControl::Subscribe(ChannelId::Sdn(SdnChannel::Worker(worker))),
            )),
        ]);

        if cfg.sdn.controller.is_some() {
            queue.push_back(WorkerInnerOutput::Bus(BusControl::Channel(
                RunnerOwner::Sdn(SdnOwner),
                BusChannelControl::Subscribe(ChannelId::Sdn(SdnChannel::Controller)),
            )));
        }

        Self {
            worker,
            sdn: SdnWorker::new(SdnWorkerCfg {
                node_id: cfg.sdn.node_id,
                tick_ms: cfg.sdn.tick_ms,
                controller: cfg.sdn.controller.map(|c| ControllerPlaneCfg {
                    session: c.session,
                    services: cfg.sdn.services.clone(),
                    authorization: c.auth,
                    handshake_builder: c.handshake,
                    random: Box::new(OsRng::default()),
                }),
                data: DataPlaneCfg {
                    worker_id: 0,
                    services: cfg.sdn.services.clone(),
                    history: cfg.sdn.history.clone(),
                },
            }),
            sfu: SfuWorker::build(worker),
            sfu_backend_slot: 0,
            sdn_backend_slot: 0,
            switcher: TaskSwitcher::new(2),
            time: TimePivot::build(),
            queue,
        }
    }

    fn worker_index(&self) -> u16 {
        self.worker
    }

    fn tasks(&self) -> usize {
        self.sdn.tasks() + self.sfu.tasks()
    }

    fn spawn(&mut self, _now: Instant, _cfg: SCfg) {
        unimplemented!()
    }

    fn on_tick(&mut self, now: Instant) {
        self.switcher.flag_all();
        let now_ms = self.time.timestamp_ms(now);
        self.sdn.on_tick(now_ms);
        self.sfu.on_tick(now);
    }

    fn on_event<'a>(&mut self, now: Instant, event: WorkerInnerInput<RunnerOwner, ExtIn, ChannelId, Event>) {
        match event {
            WorkerInnerInput::Net(owner, event) => match owner {
                RunnerOwner::Sdn(_owner) => {
                    let now_ms = self.time.timestamp_ms(now);
                    match event {
                        BackendIncoming::UdpPacket { slot: _, from, data } => {
                            self.switcher.flag_task(TaskType::Sdn as usize);
                            self.sdn.on_event(now_ms, SdnWorkerInput::Net(NetInput::UdpPacket(from, data)));
                        }
                        BackendIncoming::UdpListenResult { bind: _, result } => {
                            log::info!("Sdn listen result: {:?}", result);
                            self.sdn_backend_slot = result.expect("Should bind ok").1;
                        }
                    }
                }
                RunnerOwner::Sfu(_owner) => match event {
                    BackendIncoming::UdpPacket { slot: _, from, data } => {
                        self.switcher.flag_task(TaskType::Sfu as usize);
                        self.sfu.on_event(now, sfu::Input::UdpPacket { from, data });
                    }
                    BackendIncoming::UdpListenResult { bind: _, result } => {
                        log::info!("Sfu listen result: {:?}", result);
                        let (addr, slot) = result.expect("Should bind");
                        self.sfu_backend_slot = slot;
                        self.switcher.flag_task(TaskType::Sfu as usize);
                        self.sfu.on_event(now, sfu::Input::UdpBind { addr });
                    }
                },
            },
            WorkerInnerInput::Ext(ext) => match ext {
                ExtIn::Sdn(ext) => {
                    let now_ms = self.time.timestamp_ms(now);
                    self.switcher.flag_task(TaskType::Sdn as usize);
                    self.sdn.on_event(now_ms, SdnWorkerInput::Ext(ext));
                }
                ExtIn::HttpRequest(req) => {
                    self.switcher.flag_task(TaskType::Sfu as usize);
                    self.sfu.on_event(now, sfu::Input::HttpRequest(req));
                }
            },
            WorkerInnerInput::Bus(event) => match event {
                BusEvent::Broadcast(_from, Event::Sdn(event)) => {
                    let now_ms = self.time.timestamp_ms(now);
                    self.switcher.flag_task(TaskType::Sdn as usize);
                    self.sdn.on_event(now_ms, SdnWorkerInput::Bus(event));
                }
                BusEvent::Channel(_owner, _channel, Event::Sdn(event)) => {
                    let now_ms = self.time.timestamp_ms(now);
                    self.switcher.flag_task(TaskType::Sdn as usize);
                    self.sdn.on_event(now_ms, SdnWorkerInput::Bus(event));
                }
            },
        }
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<RunnerOwner, ExtOut, ChannelId, Event, SCfg>> {
        while let Some(current) = self.switcher.current() {
            match current.try_into().ok()? {
                TaskType::Sdn => {
                    let now_ms = self.time.timestamp_ms(now);
                    if let Some(out) = self.switcher.process(self.sdn.pop_output(now_ms)) {
                        self.process_sdn(now, out);
                        return_if_some!(self.queue.pop_front());
                    }
                }
                TaskType::Sfu => {
                    if let Some(out) = self.switcher.process(self.sfu.pop_output(now)) {
                        self.process_sfu(now, out);
                        return_if_some!(self.queue.pop_front());
                    }
                }
            }
        }

        self.queue.pop_front()
    }

    fn on_shutdown<'a>(&mut self, now: Instant) {
        self.switcher.flag_all();
        self.sfu.on_shutdown(now);
        let now_ms = self.time.timestamp_ms(now);
        self.sdn.on_event(now_ms, SdnWorkerInput::ShutdownRequest);
    }
}

impl RunnerWorker {
    fn process_sdn(&mut self, now: Instant, out: SdnWorkerOutput<(), SC, SE, TC, TW>) {
        self.switcher.flag_task(TaskType::Sdn as usize);
        match out {
            SdnWorkerOutput::Ext(ext) => self.queue.push_back(WorkerInnerOutput::Ext(true, ExtOut::Sdn(ext))),
            SdnWorkerOutput::ExtWorker(event) => match event {
                SdnExtOut::FeaturesEvent((), event) => {
                    if let FeaturesEvent::PubSub(event) = event {
                        self.sfu.on_event(now, sfu::Input::PubsubEvent(event));
                    }
                }
                SdnExtOut::ServicesEvent(..) => {}
            },
            SdnWorkerOutput::Net(out) => match out {
                NetOutput::UdpPacket(remote, data) => self.queue.push_back(WorkerInnerOutput::Net(
                    RunnerOwner::Sdn(SdnOwner),
                    BackendOutgoing::UdpPacket {
                        slot: self.sdn_backend_slot,
                        to: remote,
                        data,
                    },
                )),
                NetOutput::UdpPackets(remotes, data) => self.queue.push_back(WorkerInnerOutput::Net(
                    RunnerOwner::Sdn(SdnOwner),
                    BackendOutgoing::UdpPackets {
                        slot: self.sdn_backend_slot,
                        to: remotes,
                        data,
                    },
                )),
            },
            SdnWorkerOutput::Bus(event) => match event {
                SdnWorkerBusEvent::Control(..) => self.queue.push_back(WorkerInnerOutput::Bus(BusControl::Channel(
                    RunnerOwner::Sdn(SdnOwner),
                    BusChannelControl::Publish(ChannelId::Sdn(SdnChannel::Controller), true, event.into()),
                ))),
                SdnWorkerBusEvent::Workers(..) => self.queue.push_back(WorkerInnerOutput::Bus(BusControl::Broadcast(true, event.into()))),
                SdnWorkerBusEvent::Worker(worker, msg) => self.queue.push_back(WorkerInnerOutput::Bus(BusControl::Channel(
                    RunnerOwner::Sdn(SdnOwner),
                    BusChannelControl::Publish(ChannelId::Sdn(SdnChannel::Worker(worker)), true, Event::Sdn(SdnEvent::Worker(self.worker, msg))),
                ))),
            },
            SdnWorkerOutput::ShutdownResponse => {}
            SdnWorkerOutput::Continue => {}
        }
    }

    fn process_sfu(&mut self, now: Instant, out: sfu::Output) {
        self.switcher.flag_task(TaskType::Sfu as usize);
        match out {
            sfu::Output::HttpResponse(res) => self.queue.push_back(WorkerInnerOutput::Ext(true, ExtOut::HttpResponse(res))),
            sfu::Output::PubsubControl(control) => {
                let now_ms = self.time.timestamp_ms(now);
                self.sdn.on_event(now_ms, SdnWorkerInput::ExtWorker(SdnExtIn::FeaturesControl((), FeaturesControl::PubSub(control))));
            }
            sfu::Output::UdpPacket { to, data } => self.queue.push_back(WorkerInnerOutput::Net(
                RunnerOwner::Sdn(SdnOwner),
                BackendOutgoing::UdpPacket {
                    slot: self.sfu_backend_slot,
                    to,
                    data: data.into(),
                },
            )),
        }
    }
}
