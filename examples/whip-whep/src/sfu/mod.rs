use std::{
    collections::{hash_map::DefaultHasher, HashMap, VecDeque},
    hash::{Hash, Hasher},
    net::SocketAddr,
    time::Instant,
};

use atm0s_sdn::convert_enum;
use derive_more::Display;
use sans_io_runtime::{
    group_owner_type, NetIncoming, NetOutgoing, Task, TaskGroup, TaskGroupInput, TaskGroupOutput, TaskGroupOwner, TaskInput, TaskOutput, TaskSwitcher, WorkerInner, WorkerInnerInput, WorkerInnerOutput,
};
use str0m::{
    change::DtlsCert,
    media::{KeyframeRequestKind, MediaTime},
    rtp::{RtpHeader, RtpPacket, SeqNo},
};

use crate::http::{HttpRequest, HttpResponse};

use self::{shared_port::SharedUdpPort, whep::WhepTask, whip::WhipTask};

mod network;
mod shared_port;
mod whep;
mod whip;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum TaskId {
    Whip(usize),
    Whep(usize),
}

group_owner_type!(WhipOwner);
group_owner_type!(WhepOwner);

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, convert_enum::From)]
pub enum SfuOwner {
    Whip(WhipOwner),
    Whep(WhepOwner),
    #[convert_enum(optout)]
    System,
}

#[derive(Debug, Clone)]
pub enum ExtIn {}

#[derive(Debug, Clone)]
pub enum ExtOut {
    HttpResponse(HttpResponse),
}

#[derive(Display, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum ChannelId {
    ConsumeAudio(u64),
    ConsumeVideo(u64),
    PublishAudio(u64),
    PublishVideo(u64),
}

#[derive(Debug, Clone)]
pub struct TrackMedia {
    /// Extended sequence number to avoid having to deal with ROC.
    pub seq_no: SeqNo,

    /// Extended RTP time in the clock frequency of the codec. To avoid dealing with ROC.
    ///
    /// For a newly scheduled outgoing packet, the clock_rate is not correctly set until
    /// we do the poll_output().
    pub time: MediaTime,

    /// Parsed RTP header.
    pub header: RtpHeader,

    /// RTP payload. This contains no header.
    pub payload: Vec<u8>,

    /// str0m server timestamp.
    ///
    /// This timestamp has nothing to do with RTP itself. For outgoing packets, this is when
    /// the packet was first handed over to str0m and enqueued in the outgoing send buffers.
    /// For incoming packets it's the time we received the network packet.
    pub timestamp: Instant,
}

const SDN_TYPE: u16 = 3;

#[derive(Clone, Debug)]
pub enum SfuEvent {
    RequestKeyFrame(KeyframeRequestKind),
    Media(TrackMedia),
}

pub struct ICfg {
    pub udp_addr: SocketAddr,
}

#[derive(Debug, Clone)]
pub enum SCfg {
    HttpRequest(HttpRequest),
}

pub struct SfuWorker {
    worker: u16,
    dtls_cert: DtlsCert,
    whip_group: TaskGroup<WhipOwner, ExtIn, ExtOut, ChannelId, ChannelId, SfuEvent, SfuEvent, WhipTask, 128>,
    whep_group: TaskGroup<WhepOwner, ExtIn, ExtOut, ChannelId, ChannelId, SfuEvent, SfuEvent, WhepTask, 128>,
    output: VecDeque<WorkerInnerOutput<'static, SfuOwner, ExtOut, ChannelId, SfuEvent, SCfg>>,
    shared_udp: SharedUdpPort<TaskId>,
    switcher: TaskSwitcher,
}

impl SfuWorker {
    fn channel_build(channel: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        channel.hash(&mut hasher);
        hasher.finish()
    }

    fn process_req(&mut self, req: HttpRequest) {
        match req.path.as_str() {
            "/whip/endpoint" => self.connect_whip(req),
            "/whep/endpoint" => self.connect_whep(req),
            _ => {
                self.output.push_back(WorkerInnerOutput::Ext(
                    true,
                    ExtOut::HttpResponse(HttpResponse {
                        req_id: req.req_id,
                        status: 404,
                        headers: HashMap::new(),
                        body: b"Task Not Found".to_vec(),
                    }),
                ));
            }
        }
    }

    fn connect_whip(&mut self, req: HttpRequest) {
        let http_auth = req.http_auth();
        log::info!("Whip endpoint connect request: {}", http_auth);
        let channel = Self::channel_build(&http_auth);
        let task = WhipTask::build(
            self.shared_udp.get_backend_slot().expect(""),
            self.shared_udp.get_backend_addr().expect(""),
            self.dtls_cert.clone(),
            channel,
            &String::from_utf8_lossy(&req.body),
        );
        match task {
            Ok(task) => {
                log::info!("Whip endpoint created {}", task.ice_ufrag);
                let index = self.whip_group.add_task(task.task);
                self.shared_udp.add_ufrag(task.ice_ufrag, TaskId::Whip(index));
                self.output.push_back(WorkerInnerOutput::Ext(
                    true,
                    ExtOut::HttpResponse(HttpResponse {
                        req_id: req.req_id,
                        status: 200,
                        headers: HashMap::from([
                            ("Content-Type".to_string(), "application/sdp".to_string()),
                            ("Location".to_string(), format!("/whip/endpoint/{}/{index}", self.worker)),
                        ]),
                        body: task.sdp.into_bytes(),
                    }),
                ));
            }
            Err(err) => {
                log::error!("Error creating whip endpoint: {}", err);
                self.output.push_back(WorkerInnerOutput::Ext(
                    true,
                    ExtOut::HttpResponse(HttpResponse {
                        req_id: req.req_id,
                        status: 500,
                        headers: HashMap::new(),
                        body: err.into_bytes(),
                    }),
                ));
            }
        }
    }

    fn connect_whep(&mut self, req: HttpRequest) {
        let http_auth = req.http_auth();
        log::info!("Whep endpoint connect request: {}", http_auth);
        let channel = Self::channel_build(&http_auth);
        let task = WhepTask::build(
            self.shared_udp.get_backend_slot().expect(""),
            self.shared_udp.get_backend_addr().expect(""),
            self.dtls_cert.clone(),
            channel,
            &String::from_utf8_lossy(&req.body),
        );
        match task {
            Ok(task) => {
                log::info!("Whep endpoint created {}", task.ice_ufrag);
                let index = self.whep_group.add_task(task.task);
                self.shared_udp.add_ufrag(task.ice_ufrag, TaskId::Whep(index));
                self.output.push_back(WorkerInnerOutput::Ext(
                    true,
                    ExtOut::HttpResponse(HttpResponse {
                        req_id: req.req_id,
                        status: 200,
                        headers: HashMap::from([
                            ("Content-Type".to_string(), "application/sdp".to_string()),
                            ("Location".to_string(), format!("/whep/endpoint/{}/{index}", self.worker)),
                        ]),
                        body: task.sdp.into_bytes(),
                    }),
                ));
            }
            Err(err) => {
                log::error!("Error creating whep endpoint: {}", err);
                self.output.push_back(WorkerInnerOutput::Ext(
                    true,
                    ExtOut::HttpResponse(HttpResponse {
                        req_id: req.req_id,
                        status: 500,
                        headers: HashMap::new(),
                        body: err.into_bytes(),
                    }),
                ));
            }
        }
    }
}

impl WorkerInner<SfuOwner, ExtIn, ExtOut, ChannelId, SfuEvent, ICfg, SCfg> for SfuWorker {
    fn build(worker: u16, cfg: ICfg) -> Self {
        Self {
            worker,
            dtls_cert: DtlsCert::new_openssl(),
            whip_group: TaskGroup::new(),
            whep_group: TaskGroup::new(),
            output: VecDeque::from([WorkerInnerOutput::Task(SfuOwner::System, TaskOutput::Net(NetOutgoing::UdpListen { addr: cfg.udp_addr, reuse: false }))]),
            shared_udp: SharedUdpPort::default(),
            switcher: TaskSwitcher::new(2),
        }
    }
    fn worker_index(&self) -> u16 {
        self.worker
    }
    fn tasks(&self) -> usize {
        self.whip_group.tasks() + self.whep_group.tasks()
    }
    fn spawn(&mut self, _now: Instant, cfg: SCfg) {
        match cfg {
            SCfg::HttpRequest(req) => {
                self.process_req(req);
            }
        }
    }
    fn on_tick<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SfuOwner, ExtOut, ChannelId, SfuEvent, SCfg>> {
        if let Some(e) = self.output.pop_front() {
            return Some(e.into());
        }

        let switcher = &mut self.switcher;
        loop {
            match switcher.looper_current(now)? as u16 {
                WhipTask::TYPE => {
                    if let Some(res) = switcher.looper_process(self.whip_group.on_tick(now)) {
                        if matches!(res.1, TaskOutput::Destroy) {
                            self.shared_udp.remove_task(TaskId::Whip(res.0.task_index()));
                        }
                        return Some(res.into());
                    }
                }
                WhepTask::TYPE => {
                    if let Some(res) = switcher.looper_process(self.whep_group.on_tick(now)) {
                        if matches!(res.1, TaskOutput::Destroy) {
                            self.shared_udp.remove_task(TaskId::Whip(res.0.task_index()));
                        }
                        return Some(res.into());
                    }
                }
                _ => panic!("Unknown task type"),
            }
        }
    }

    fn on_event<'a>(&mut self, now: Instant, event: WorkerInnerInput<'a, SfuOwner, ExtIn, ChannelId, SfuEvent>) -> Option<WorkerInnerOutput<'a, SfuOwner, ExtOut, ChannelId, SfuEvent, SCfg>> {
        match event {
            WorkerInnerInput::Task(owner, event) => match event {
                TaskInput::Net(NetIncoming::UdpListenResult { bind: _, result }) => {
                    log::info!("UdpListenResult: {:?}", result);
                    let (addr, slot) = result.as_ref().expect("Should listen shared port ok");
                    self.shared_udp.set_backend_info(*addr, *slot);
                    None
                }
                TaskInput::Net(NetIncoming::UdpPacket { from, slot, data }) => match self.shared_udp.map_remote(from, &data)? {
                    TaskId::Whip(index) => {
                        self.switcher.queue_flag_task(WhipTask::TYPE as usize);
                        let owner = WhipOwner(index);
                        let TaskGroupOutput(owner, output) = self.whip_group.on_event(now, TaskGroupInput(owner, TaskInput::Net(NetIncoming::UdpPacket { from, slot, data })))?;
                        Some(WorkerInnerOutput::Task(owner.into(), output))
                    }
                    TaskId::Whep(index) => {
                        self.switcher.queue_flag_task(WhepTask::TYPE as usize);
                        let owner = WhepOwner(index);
                        let TaskGroupOutput(owner, output) = self.whep_group.on_event(now, TaskGroupInput(owner, TaskInput::Net(NetIncoming::UdpPacket { from, slot, data })))?;
                        Some(WorkerInnerOutput::Task(owner.into(), output))
                    }
                },
                TaskInput::Bus(channel, event) => match owner {
                    SfuOwner::Whip(owner) => {
                        let TaskGroupOutput(owner, output) = self.whip_group.on_event(now, TaskGroupInput(owner, TaskInput::Bus(channel, event)))?;
                        Some(WorkerInnerOutput::Task(owner.into(), output))
                    }
                    SfuOwner::Whep(owner) => {
                        let TaskGroupOutput(owner, output) = self.whep_group.on_event(now, TaskGroupInput(owner, TaskInput::Bus(channel, event)))?;
                        Some(WorkerInnerOutput::Task(owner.into(), output))
                    }
                    _ => None,
                },
                _ => None,
            },
            WorkerInnerInput::Ext(_ext) => None,
        }
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SfuOwner, ExtOut, ChannelId, SfuEvent, SCfg>> {
        let switcher = &mut self.switcher;
        while let Some(current) = switcher.queue_current() {
            match current as u16 {
                WhipTask::TYPE => {
                    if let Some(out) = switcher.queue_process(self.whip_group.pop_output(now).map(|o| o.into())) {
                        return Some(out);
                    }
                }
                WhepTask::TYPE => {
                    if let Some(out) = switcher.queue_process(self.whep_group.pop_output(now).map(|o| o.into())) {
                        return Some(out);
                    }
                }
                _ => panic!("Should not called"),
            }
        }
        None
    }

    fn shutdown<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SfuOwner, ExtOut, ChannelId, SfuEvent, SCfg>> {
        let switcher = &mut self.switcher;
        loop {
            match switcher.looper_current(now)? as u16 {
                WhipTask::TYPE => {
                    if let Some(res) = switcher.looper_process(self.whip_group.shutdown(now)) {
                        if matches!(res.1, TaskOutput::Destroy) {
                            self.shared_udp.remove_task(TaskId::Whip(res.0.task_index()));
                        }
                        return Some(res.into());
                    }
                }
                WhepTask::TYPE => {
                    if let Some(res) = switcher.looper_process(self.whep_group.shutdown(now)) {
                        if matches!(res.1, TaskOutput::Destroy) {
                            self.shared_udp.remove_task(TaskId::Whip(res.0.task_index()));
                        }
                        return Some(res.into());
                    }
                }
                _ => panic!("Unknown task type"),
            }
        }
    }
}

impl TrackMedia {
    pub fn from_raw(rtp: RtpPacket) -> Self {
        let header = rtp.header;
        let payload = rtp.payload;
        let time = rtp.time;
        let timestamp = rtp.timestamp;
        let seq_no = rtp.seq_no;

        Self {
            seq_no,
            time,
            header,
            payload,
            timestamp,
        }
    }
}
