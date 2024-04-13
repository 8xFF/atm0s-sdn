//!
//! This is common mode for testing perpose
//! We will create a node with a controller and single worker, which is enough for testing
//!

use std::collections::HashMap;
use std::fmt::Debug;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::{collections::VecDeque, net::IpAddr};

use atm0s_sdn_identity::{NodeAddr, NodeAddrBuilder, NodeId, Protocol};
use atm0s_sdn_network::base::ServiceBuilder;
use atm0s_sdn_network::controller_plane::ControllerPlaneCfg;
use atm0s_sdn_network::data_plane::DataPlaneCfg;
use atm0s_sdn_network::features::{FeaturesControl, FeaturesEvent};
use atm0s_sdn_network::secure::{HandshakeBuilderXDA, StaticKeyAuthorization};
use atm0s_sdn_network::worker::{SdnWorker, SdnWorkerCfg, SdnWorkerInput, SdnWorkerOutput};
use atm0s_sdn_network::{
    base::{Buffer, BufferMut},
    data_plane, ExtIn, ExtOut,
};
use atm0s_sdn_router::shadow::ShadowRouterHistory;
use log::{LevelFilter, Metadata, Record};
use parking_lot::Mutex;
use rand::rngs::mock::StepRng;
use sans_io_runtime::TaskSwitcher;

static CONTEXT_LOGGER: ContextLogger = ContextLogger { node: Mutex::new(None) };

struct ContextLogger {
    node: Mutex<Option<NodeId>>,
}

impl ContextLogger {
    pub fn set_ctx(&self, node: NodeId) {
        *self.node.lock() = Some(node);
    }

    pub fn clear_ctx(&self) {
        *self.node.lock() = None;
    }
}

impl log::Log for ContextLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            if let Some(node) = self.node.lock().as_ref() {
                println!("[Node {}] {} - {}", node, record.level(), record.args());
            } else {
                println!("[------] {} - {}", record.level(), record.args());
            }
        }
    }
    fn flush(&self) {}
}

struct AutoContext();
impl AutoContext {
    pub fn new(node: NodeId) -> Self {
        CONTEXT_LOGGER.set_ctx(node);
        Self()
    }
}

impl Drop for AutoContext {
    fn drop(&mut self) {
        CONTEXT_LOGGER.clear_ctx();
    }
}

#[derive(Debug)]
pub enum TestNodeIn<'a, SC> {
    Ext(ExtIn<SC>),
    ExtWorker(ExtIn<SC>),
    Udp(SocketAddr, BufferMut<'a>),
    #[cfg(feature = "vpn")]
    Tun(BufferMut<'a>),
}

#[derive(Debug)]
pub enum TestNodeOut<'a, SE> {
    Ext(ExtOut<SE>),
    ExtWorker(ExtOut<SE>),
    Udp(Vec<SocketAddr>, Buffer<'a>),
    #[cfg(feature = "vpn")]
    Tun(Buffer<'a>),
    Continue,
}

pub fn build_addr(node_id: NodeId) -> NodeAddr {
    let mut builder = NodeAddrBuilder::new(node_id);
    builder.add_protocol(Protocol::Ip4(Ipv4Addr::LOCALHOST));
    builder.add_protocol(Protocol::Udp(node_id as u16));
    builder.addr()
}

#[derive(Debug, Default)]
struct SingleThreadDataWorkerHistory {
    queue: Mutex<Vec<(Option<NodeId>, u8, u16)>>,
    map: Mutex<HashMap<(Option<NodeId>, u8, u16), bool>>,
}

impl ShadowRouterHistory for SingleThreadDataWorkerHistory {
    fn already_received_broadcast(&self, from: Option<NodeId>, service: u8, seq: u16) -> bool {
        let mut map = self.map.lock();
        let mut queue = self.queue.lock();
        if map.contains_key(&(from, service, seq)) {
            log::debug!("already_received_broadcast from {:?} service {} seq {}", from, service, seq);
            return true;
        }
        map.insert((from, service, seq), true);
        if queue.len() > 100 {
            let pair = queue.remove(0);
            map.remove(&pair);
        }
        false
    }

    fn set_ts(&self, _now: u64) {}
}

pub struct TestNode<SC, SE, TC, TW> {
    node_id: NodeId,
    worker: SdnWorker<SC, SE, TC, TW>,
}

impl<SC: Debug, SE: Debug, TC: Debug, TW: Debug> TestNode<SC, SE, TC, TW> {
    pub fn new(node_id: NodeId, session: u64, services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, SC, SE, TC, TW>>>) -> Self {
        let _log = AutoContext::new(node_id);
        let authorization: Arc<StaticKeyAuthorization> = Arc::new(StaticKeyAuthorization::new("demo-key"));
        let handshake_builder = Arc::new(HandshakeBuilderXDA);
        let random = Box::new(StepRng::new(1000, 5));
        Self {
            node_id,
            worker: SdnWorker::new(SdnWorkerCfg {
                node_id,
                tick_ms: 1,
                controller: Some(ControllerPlaneCfg {
                    session,
                    services: services.clone(),
                    authorization,
                    handshake_builder,
                    random,
                }),
                data: DataPlaneCfg {
                    worker_id: 0,
                    services,
                    history: Arc::new(SingleThreadDataWorkerHistory::default()),
                },
            }),
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn addr(&self) -> NodeAddr {
        build_addr(self.node_id)
    }

    pub fn tick<'a>(&mut self, now: u64) -> Option<TestNodeOut<'a, SE>> {
        let _log = AutoContext::new(self.node_id);
        let out = self.worker.on_tick(now)?;
        Some(self.process_worker_output(now, out))
    }

    pub fn on_input<'a>(&mut self, now: u64, input: TestNodeIn<'a, SC>) -> Option<TestNodeOut<'a, SE>> {
        let _log = AutoContext::new(self.node_id);
        match input {
            TestNodeIn::Ext(ext_in) => {
                let out = self.worker.on_event(now, SdnWorkerInput::Ext(ext_in))?;
                Some(self.process_worker_output(now, out))
            }
            TestNodeIn::ExtWorker(ext_in) => {
                let out = self.worker.on_event(now, SdnWorkerInput::ExtWorker(ext_in))?;
                Some(self.process_worker_output(now, out))
            }
            TestNodeIn::Udp(addr, buf) => {
                let out = self.worker.on_event(now, SdnWorkerInput::Net(data_plane::NetInput::UdpPacket(addr, buf)))?;
                Some(self.process_worker_output(now, out))
            }
            #[cfg(feature = "vpn")]
            TestNodeIn::Tun(buf) => {
                let out = self.worker.on_event(now, SdnWorkerInput::Net(data_plane::NetInput::TunPacket(buf)))?;
                Some(self.process_worker_output(now, out))
            }
        }
    }

    pub fn pop_output<'a>(&mut self, now: u64) -> Option<TestNodeOut<'a, SE>> {
        let _log = AutoContext::new(self.node_id);
        let output = self.worker.pop_output(now)?;
        Some(self.process_worker_output(now, output))
    }

    fn process_worker_output<'a>(&mut self, now: u64, output: SdnWorkerOutput<'a, SC, SE, TC, TW>) -> TestNodeOut<'a, SE> {
        match output {
            SdnWorkerOutput::Ext(ext) => TestNodeOut::Ext(ext),
            SdnWorkerOutput::ExtWorker(ext) => TestNodeOut::ExtWorker(ext),
            SdnWorkerOutput::Net(data_plane::NetOutput::UdpPacket(dest, data)) => TestNodeOut::Udp(vec![dest], data),
            SdnWorkerOutput::Net(data_plane::NetOutput::UdpPackets(dests, data)) => TestNodeOut::Udp(dests, data),
            #[cfg(feature = "vpn")]
            SdnWorkerOutput::Net(data_plane::NetOutput::TunPacket(data)) => TestNodeOut::Tun(data),
            SdnWorkerOutput::Bus(bus) => {
                if let Some(out) = self.worker.on_event(now, SdnWorkerInput::Bus(bus)) {
                    self.process_worker_output(now, out)
                } else {
                    TestNodeOut::Continue
                }
            }
            SdnWorkerOutput::ShutdownResponse => todo!(),
            SdnWorkerOutput::Continue => TestNodeOut::Continue,
        }
    }
}

pub fn addr_to_node(addr: SocketAddr) -> NodeId {
    addr.port() as u32
}

pub fn node_to_addr(node: NodeId) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), node as u16)
}

pub struct NetworkSimulator<SC, SE, TC: Clone, TW: Clone> {
    clock_ms: u64,
    input: VecDeque<(NodeId, ExtIn<SC>)>,
    input_worker: VecDeque<(NodeId, ExtIn<SC>)>,
    output: VecDeque<(NodeId, ExtOut<SE>)>,
    output_worker: VecDeque<(NodeId, ExtOut<SE>)>,
    nodes: Vec<TestNode<SC, SE, TC, TW>>,
    nodes_index: HashMap<NodeId, usize>,
    switcher: TaskSwitcher,
}

impl<SC: Debug, SE: Debug, TC: Debug + Clone, TW: Debug + Clone> NetworkSimulator<SC, SE, TC, TW> {
    pub fn new(started_ms: u64) -> Self {
        Self {
            clock_ms: started_ms,
            input: VecDeque::new(),
            output: VecDeque::new(),
            input_worker: VecDeque::new(),
            output_worker: VecDeque::new(),
            nodes: Vec::new(),
            nodes_index: HashMap::new(),
            switcher: TaskSwitcher::new(0),
        }
    }

    #[allow(unused)]
    pub fn enable_log(&self, level: LevelFilter) {
        log::set_logger(&CONTEXT_LOGGER).expect("Should set global logger");
        log::set_max_level(level);
    }

    pub fn control(&mut self, node: NodeId, control: ExtIn<SC>) {
        self.input.push_back((node, control));
    }

    pub fn pop_res(&mut self) -> Option<(NodeId, ExtOut<SE>)> {
        self.output.pop_front()
    }

    pub fn control_worker(&mut self, node: NodeId, control: ExtIn<SC>) {
        self.input_worker.push_back((node, control));
    }

    pub fn pop_res_worker(&mut self) -> Option<(NodeId, ExtOut<SE>)> {
        self.output_worker.pop_front()
    }

    pub fn add_node(&mut self, node: TestNode<SC, SE, TC, TW>) -> NodeAddr {
        let index = self.nodes.len();
        self.nodes_index.insert(node.node_id(), index);
        let addr = node.addr();
        self.nodes.push(node);
        self.switcher.set_tasks(self.nodes.len());
        addr
    }

    pub fn process(&mut self, delta: u64) {
        self.clock_ms += delta;
        log::debug!("Tick {} ms", self.clock_ms);
        for i in 0..self.nodes.len() {
            let node_id = self.nodes[i].node_id();
            if let Some(out) = self.nodes[i].tick(self.clock_ms) {
                self.process_out(self.clock_ms, node_id, out);
            }
        }

        while let Some((node, input)) = self.input.pop_front() {
            let node_index = *self.nodes_index.get(&node).expect("Node not found");
            if let Some(out) = self.nodes[node_index].on_input(self.clock_ms, TestNodeIn::Ext(input)) {
                self.process_out(self.clock_ms, node, out);
            }
        }

        while let Some((node, input)) = self.input_worker.pop_front() {
            let node_index = *self.nodes_index.get(&node).expect("Node not found");
            if let Some(out) = self.nodes[node_index].on_input(self.clock_ms, TestNodeIn::ExtWorker(input)) {
                self.process_out(self.clock_ms, node, out);
            }
        }

        self.switcher.queue_flag_all();
        self.pop_outputs(self.clock_ms);
    }

    fn pop_outputs(&mut self, now: u64) {
        while let Some(index) = self.switcher.queue_current() {
            let node = self.nodes[index].node_id();
            if let Some(out) = self.switcher.queue_process(self.nodes[index].pop_output(now)) {
                self.process_out(now, node, out);
            }
        }
    }

    fn process_out(&mut self, now: u64, node: NodeId, out: TestNodeOut<SE>) {
        let node_index = *self.nodes_index.get(&node).expect("Node not found");
        self.switcher.queue_flag_task(node_index);
        match out {
            TestNodeOut::Ext(out) => {
                self.output.push_back((node, out));
            }
            TestNodeOut::ExtWorker(out) => {
                self.output_worker.push_back((node, out));
            }
            TestNodeOut::Udp(dests, data) => {
                let source_addr = node_to_addr(node);
                for dest in dests {
                    log::debug!("Send UDP packet from {} to {}, buf len {}", source_addr, dest, data.len());
                    let dest_node = addr_to_node(dest);
                    let dest_index = *self.nodes_index.get(&dest_node).expect("Node not found");
                    if let Some(out) = self.nodes[dest_index].on_input(now, TestNodeIn::Udp(source_addr, data.clone_mut())) {
                        self.process_out(now, dest_node, out);
                    }
                }
            }
            #[cfg(feature = "vpn")]
            TestNodeOut::Tun(_) => todo!(),
            TestNodeOut::Continue => {}
        }
    }
}
