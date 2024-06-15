use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    time::Instant,
};

use atm0s_sdn::sans_io_runtime::{group_owner_type, group_task, return_if_some, Buffer, TaskGroup, TaskSwitcher};

use atm0s_sdn::features::pubsub;
use str0m::change::DtlsCert;

use crate::http::{HttpRequest, HttpResponse};

use self::{
    cluster::ClusterLogic,
    media::TrackMedia,
    shared_port::SharedUdpPort,
    whep::{WhepInput, WhepOutput, WhepTask},
    whip::{WhipInput, WhipOutput, WhipTask},
};

mod cluster;
mod media;
mod shared_port;
mod whep;
mod whip;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct SfuChannel;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
enum TaskId {
    Whip(usize),
    Whep(usize),
}

#[derive(Debug, Clone)]
pub enum Input {
    HttpRequest(HttpRequest),
    PubsubEvent(pubsub::Event),
    UdpBind { addr: SocketAddr },
    UdpPacket { from: std::net::SocketAddr, data: Buffer },
}

#[derive(Debug, Clone)]
pub enum Output {
    HttpResponse(HttpResponse),
    PubsubControl(pubsub::Control),
    UdpPacket { to: std::net::SocketAddr, data: Buffer },
}

group_owner_type!(WhipOwner);
group_owner_type!(WhepOwner);
group_task!(WhepTaskGroup, WhepTask, WhepInput<'a>, WhepOutput);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SfuOwner;

pub struct SfuWorker {
    worker: u16,
    dtls_cert: DtlsCert,
    cluster: ClusterLogic,
    whip_group: TaskGroup<WhipInput, WhipOutput, WhipTask, 16>,
    whep_group: WhepTaskGroup,
    output: VecDeque<Output>,
    shared_udp: SharedUdpPort<TaskId>,
    switcher: TaskSwitcher,
}

impl SfuWorker {
    fn process_req(&mut self, req: HttpRequest) {
        match req.path.as_str() {
            "/whip/endpoint" => self.connect_whip(req),
            "/whep/endpoint" => self.connect_whep(req),
            _ => {
                self.output.push_back(Output::HttpResponse(HttpResponse {
                    req_id: req.req_id,
                    status: 404,
                    headers: HashMap::new(),
                    body: b"Task Not Found".to_vec(),
                }));
            }
        }
    }

    fn connect_whip(&mut self, req: HttpRequest) {
        let http_auth = req.http_auth();
        log::info!("Whip endpoint connect request: {}", http_auth);
        let room = http_auth;
        let task = WhipTask::build(self.shared_udp.get_backend_addr().expect(""), self.dtls_cert.clone(), room, &String::from_utf8_lossy(&req.body));
        match task {
            Ok(task) => {
                log::info!("Whip endpoint created {}", task.ice_ufrag);
                let index = self.whip_group.add_task(task.task);
                self.shared_udp.add_ufrag(task.ice_ufrag, TaskId::Whip(index));
                self.output.push_back(Output::HttpResponse(HttpResponse {
                    req_id: req.req_id,
                    status: 200,
                    headers: HashMap::from([
                        ("Content-Type".to_string(), "application/sdp".to_string()),
                        ("Location".to_string(), format!("/whip/endpoint/{}/{index}", self.worker)),
                    ]),
                    body: task.sdp.into_bytes(),
                }));
            }
            Err(err) => {
                log::error!("Error creating whip endpoint: {}", err);
                self.output.push_back(Output::HttpResponse(HttpResponse {
                    req_id: req.req_id,
                    status: 500,
                    headers: HashMap::new(),
                    body: err.into_bytes(),
                }));
            }
        }
    }

    fn connect_whep(&mut self, req: HttpRequest) {
        let http_auth = req.http_auth();
        log::info!("Whep endpoint connect request: {}", http_auth);
        let room = http_auth;
        let task = WhepTask::build(self.shared_udp.get_backend_addr().expect(""), self.dtls_cert.clone(), room, &String::from_utf8_lossy(&req.body));
        match task {
            Ok(task) => {
                log::info!("Whep endpoint created {}", task.ice_ufrag);
                let index = self.whep_group.add_task(task.task);
                self.shared_udp.add_ufrag(task.ice_ufrag, TaskId::Whep(index));
                self.output.push_back(Output::HttpResponse(HttpResponse {
                    req_id: req.req_id,
                    status: 200,
                    headers: HashMap::from([
                        ("Content-Type".to_string(), "application/sdp".to_string()),
                        ("Location".to_string(), format!("/whep/endpoint/{}/{index}", self.worker)),
                    ]),
                    body: task.sdp.into_bytes(),
                }));
            }
            Err(err) => {
                log::error!("Error creating whep endpoint: {}", err);
                self.output.push_back(Output::HttpResponse(HttpResponse {
                    req_id: req.req_id,
                    status: 500,
                    headers: HashMap::new(),
                    body: err.into_bytes(),
                }));
            }
        }
    }
}

#[repr(u8)]
enum TaskType {
    Cluster = 0,
    Whip = 1,
    Whep = 2,
}

impl From<usize> for TaskType {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::Cluster,
            1 => Self::Whip,
            2 => Self::Whep,
            _ => panic!("Should not happen"),
        }
    }
}

impl SfuWorker {
    fn process_cluster_output(&mut self, now: Instant, out: cluster::Output) {
        self.switcher.flag_task(TaskType::Cluster as usize);
        match out {
            cluster::Output::Pubsub(control) => self.output.push_back(Output::PubsubControl(control)),
            cluster::Output::WhepMedia(owners, media) => {
                self.switcher.flag_task(TaskType::Whep as usize);
                for owner in owners {
                    self.whep_group.on_event(now, owner.index(), WhepInput::Media(&media));
                }
            }
            cluster::Output::WhipControl(owners, kind) => {
                self.switcher.flag_task(TaskType::Whip as usize);
                for owner in owners {
                    self.whip_group.on_event(now, owner.index(), WhipInput::KeyFrame(kind));
                }
            }
        }
    }

    fn process_whip_out(&mut self, now: Instant, index: usize, out: WhipOutput) {
        self.switcher.flag_task(TaskType::Whip as usize);
        match out {
            WhipOutput::Started(room) => {
                if let Some(out) = self.cluster.on_input(now, cluster::Input::WhipStart(WhipOwner(index), room)) {
                    self.process_cluster_output(now, out)
                }
            }
            WhipOutput::Media(media) => {
                if let Some(out) = self.cluster.on_input(now, cluster::Input::WhipMedia(WhipOwner(index), media)) {
                    self.process_cluster_output(now, out)
                }
            }
            WhipOutput::UdpPacket { to, data } => self.output.push_back(Output::UdpPacket { to, data }),
            WhipOutput::Destroy => {
                self.shared_udp.remove_task(TaskId::Whip(index));
                self.whip_group.remove_task(index);
                log::info!("destroy whip({index}) => remain {}", self.whip_group.tasks());
                if let Some(out) = self.cluster.on_input(now, cluster::Input::WhipStop(WhipOwner(index))) {
                    self.process_cluster_output(now, out);
                }
            }
        }
    }

    fn process_whep_out(&mut self, now: Instant, index: usize, out: WhepOutput) {
        self.switcher.flag_task(TaskType::Whep as usize);
        match out {
            WhepOutput::Started(room) => {
                if let Some(out) = self.cluster.on_input(now, cluster::Input::WhepStart(WhepOwner(index), room)) {
                    self.process_cluster_output(now, out);
                }
            }
            WhepOutput::RequestKey(kind) => {
                if let Some(out) = self.cluster.on_input(now, cluster::Input::WhepRequest(WhepOwner(index), kind)) {
                    self.process_cluster_output(now, out);
                }
            }
            WhepOutput::UdpPacket { to, data } => {
                self.output.push_back(Output::UdpPacket { to, data });
            }
            WhepOutput::Destroy => {
                self.shared_udp.remove_task(TaskId::Whip(index));
                self.whep_group.remove_task(index);
                log::info!("destroy whep({index}) => remain {}", self.whep_group.tasks());
                if let Some(out) = self.cluster.on_input(now, cluster::Input::WhepStop(WhepOwner(index))) {
                    self.process_cluster_output(now, out);
                }
            }
        }
    }
}

impl SfuWorker {
    pub fn build(worker: u16) -> Self {
        Self {
            worker,
            dtls_cert: DtlsCert::new_openssl(),
            cluster: ClusterLogic::default(),
            whip_group: TaskGroup::default(),
            whep_group: WhepTaskGroup::default(),
            shared_udp: SharedUdpPort::default(),
            switcher: TaskSwitcher::new(3),
            output: VecDeque::new(),
        }
    }
    pub fn worker_index(&self) -> u16 {
        self.worker
    }
    pub fn tasks(&self) -> usize {
        self.whip_group.tasks() + self.whep_group.tasks()
    }
    pub fn on_tick(&mut self, now: Instant) {
        self.switcher.flag_all();
        self.whip_group.on_tick(now);
        self.whep_group.on_tick(now);
    }

    pub fn on_event(&mut self, now: Instant, input: Input) {
        match input {
            Input::UdpBind { addr } => {
                log::info!("UdpBind: {}", addr);
                self.shared_udp.set_backend_info(addr);
            }
            Input::UdpPacket { from, data } => match self.shared_udp.map_remote(from, &data) {
                Some(TaskId::Whip(index)) => {
                    self.switcher.flag_task(TaskType::Whip as usize);
                    self.whip_group.on_event(now, index, WhipInput::UdpPacket { from, data });
                }
                Some(TaskId::Whep(index)) => {
                    self.switcher.flag_task(TaskType::Whep as usize);
                    self.whep_group.on_event(now, index, WhepInput::UdpPacket { from, data });
                }
                None => {
                    log::debug!("Unknown remote address: {}", from);
                }
            },
            Input::HttpRequest(req) => {
                self.process_req(req);
            }
            Input::PubsubEvent(event) => {
                if let Some(out) = self.cluster.on_input(now, cluster::Input::Pubsub(event)) {
                    self.process_cluster_output(now, out)
                }
            }
        }
    }

    pub fn pop_output(&mut self, now: Instant) -> Option<Output> {
        return_if_some!(self.output.pop_front());

        while let Some(current) = self.switcher.current() {
            match current.into() {
                TaskType::Cluster => {
                    self.switcher.process(None::<()>);
                    continue;
                }
                TaskType::Whip => {
                    let out = self.whip_group.pop_output(now);
                    if let Some((index, out)) = self.switcher.process(out) {
                        self.process_whip_out(now, index, out);
                        return_if_some!(self.output.pop_front());
                    }
                }
                TaskType::Whep => {
                    let out = self.whep_group.pop_output(now);
                    if let Some((index, out)) = self.switcher.process(out) {
                        self.process_whep_out(now, index, out);
                        return_if_some!(self.output.pop_front());
                    }
                }
            }
        }
        None
    }

    pub fn on_shutdown(&mut self, now: Instant) {
        self.switcher.flag_all();
        self.whip_group.on_shutdown(now);
        self.whep_group.on_shutdown(now);
    }
}
