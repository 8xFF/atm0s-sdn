use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_router::{shadow::ShadowRouter, RouteRule};
use sans_io_runtime::TaskSwitcherChild;

use crate::data_plane::NetPair;

use super::{Buffer, ConnectionCtx, ConnectionEvent, ServiceId, TransportMsgHeader, Ttl};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NetIncomingMeta {
    pub source: Option<NodeId>,
    pub ttl: Ttl,
    pub meta: u8,
    pub secure: bool,
}

impl NetIncomingMeta {
    pub fn new(source: Option<NodeId>, ttl: Ttl, meta: u8, secure: bool) -> Self {
        Self { source, ttl, meta, secure }
    }
}

impl From<&TransportMsgHeader> for NetIncomingMeta {
    fn from(value: &TransportMsgHeader) -> Self {
        Self {
            source: value.from_node,
            ttl: Ttl(value.ttl),
            meta: value.meta,
            secure: value.encrypt,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NetOutgoingMeta {
    pub source: bool,
    pub ttl: Ttl,
    pub meta: u8,
    pub secure: bool,
}

impl NetOutgoingMeta {
    pub fn new(source: bool, ttl: Ttl, meta: u8, secure: bool) -> Self {
        Self { source, ttl, meta, secure }
    }

    pub fn secure() -> Self {
        Self {
            source: false,
            ttl: Ttl::default(),
            meta: 0,
            secure: true,
        }
    }

    pub fn to_header(&self, feature: u8, rule: RouteRule, node_id: NodeId) -> TransportMsgHeader {
        TransportMsgHeader::build(feature, self.meta, rule)
            .set_ttl(*self.ttl)
            .set_from_node(if self.source {
                Some(node_id)
            } else {
                None
            })
            .set_encrypt(self.secure)
    }

    pub fn to_incoming(&self, node_id: NodeId) -> NetIncomingMeta {
        NetIncomingMeta {
            source: if self.source {
                Some(node_id)
            } else {
                None
            },
            ttl: self.ttl,
            meta: self.meta,
            secure: self.secure,
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum FeatureControlActor<UserData> {
    Controller(UserData),
    Worker(u16, UserData),
    Service(ServiceId),
}

impl<UserData> FeatureControlActor<UserData> {
    pub fn into2<UserData2>(self) -> FeatureControlActor<UserData2>
    where
        UserData2: From<UserData>,
    {
        match self {
            FeatureControlActor::Controller(u) => FeatureControlActor::Controller(u.into()),
            FeatureControlActor::Worker(worker, u) => FeatureControlActor::Worker(worker, u.into()),
            FeatureControlActor::Service(service) => FeatureControlActor::Service(service),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FeatureSharedInput {
    Tick(u64),
    Connection(ConnectionEvent),
}

#[derive(Debug, Clone)]
pub enum FeatureInput<'a, UserData, Control, ToController> {
    FromWorker(ToController),
    Control(FeatureControlActor<UserData>, Control),
    Net(&'a ConnectionCtx, NetIncomingMeta, Buffer),
    Local(NetIncomingMeta, Buffer),
}

#[derive(Debug, PartialEq, Eq)]
pub enum FeatureOutput<UserData, Event, ToWorker> {
    /// First bool is flag for broadcast or not
    ToWorker(bool, ToWorker),
    Event(FeatureControlActor<UserData>, Event),
    SendDirect(ConnId, NetOutgoingMeta, Buffer),
    SendRoute(RouteRule, NetOutgoingMeta, Buffer),
    NeighboursConnectTo(NodeAddr),
    NeighboursDisconnectFrom(NodeId),
}

impl<UserData, Event, ToWorker> FeatureOutput<UserData, Event, ToWorker> {
    pub fn into2<UserData2, Event2, ToWorker2>(self) -> FeatureOutput<UserData2, Event2, ToWorker2>
    where
        UserData2: From<UserData>,
        Event2: From<Event>,
        ToWorker2: From<ToWorker>,
    {
        match self {
            FeatureOutput::ToWorker(is_broadcast, to) => FeatureOutput::ToWorker(is_broadcast, to.into()),
            FeatureOutput::Event(actor, event) => FeatureOutput::Event(actor.into2(), event.into()),
            FeatureOutput::SendDirect(conn, meta, msg) => FeatureOutput::SendDirect(conn, meta, msg),
            FeatureOutput::SendRoute(rule, ttl, buf) => FeatureOutput::SendRoute(rule, ttl, buf),
            FeatureOutput::NeighboursConnectTo(addr) => FeatureOutput::NeighboursConnectTo(addr),
            FeatureOutput::NeighboursDisconnectFrom(id) => FeatureOutput::NeighboursDisconnectFrom(id),
        }
    }
}

pub struct FeatureContext {
    pub node_id: NodeId,
    pub session: u64,
}

pub trait Feature<UserData, Control, Event, ToController, ToWorker>: TaskSwitcherChild<FeatureOutput<UserData, Event, ToWorker>> {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, _now: u64, _input: FeatureSharedInput);
    fn on_input(&mut self, _ctx: &FeatureContext, now_ms: u64, input: FeatureInput<'_, UserData, Control, ToController>);
}

pub enum FeatureWorkerInput<UserData, Control, ToWorker> {
    /// First bool is flag for broadcast or not
    FromController(bool, ToWorker),
    Control(FeatureControlActor<UserData>, Control),
    Network(ConnId, NetIncomingMeta, Buffer),
    Local(NetIncomingMeta, Buffer),
    #[cfg(feature = "vpn")]
    TunPkt(Buffer),
}

#[derive(Debug, Clone)]
pub enum FeatureWorkerOutput<UserData, Control, Event, ToController> {
    ForwardControlToController(FeatureControlActor<UserData>, Control),
    ForwardNetworkToController(ConnId, NetIncomingMeta, Buffer),
    ForwardLocalToController(NetIncomingMeta, Buffer),
    ToController(ToController),
    Event(FeatureControlActor<UserData>, Event),
    SendDirect(ConnId, NetOutgoingMeta, Buffer),
    SendRoute(RouteRule, NetOutgoingMeta, Buffer),
    RawDirect(ConnId, Buffer),
    RawBroadcast(Vec<ConnId>, Buffer),
    RawDirect2(NetPair, Buffer),
    RawBroadcast2(Vec<NetPair>, Buffer),
    #[cfg(feature = "vpn")]
    TunPkt(Buffer),
}

impl<UserData, Control, Event, ToController> FeatureWorkerOutput<UserData, Control, Event, ToController> {
    pub fn into2<UserData2, Control2, Event2, ToController2>(self) -> FeatureWorkerOutput<UserData2, Control2, Event2, ToController2>
    where
        UserData2: From<UserData>,
        Control2: From<Control>,
        Event2: From<Event>,
        ToController2: From<ToController>,
    {
        match self {
            FeatureWorkerOutput::ForwardControlToController(actor, control) => FeatureWorkerOutput::ForwardControlToController(actor.into2(), control.into()),
            FeatureWorkerOutput::ForwardNetworkToController(conn, header, msg) => FeatureWorkerOutput::ForwardNetworkToController(conn, header, msg),
            FeatureWorkerOutput::ForwardLocalToController(header, buf) => FeatureWorkerOutput::ForwardLocalToController(header, buf),
            FeatureWorkerOutput::ToController(to) => FeatureWorkerOutput::ToController(to.into()),
            FeatureWorkerOutput::Event(actor, event) => FeatureWorkerOutput::Event(actor.into2(), event.into()),
            FeatureWorkerOutput::SendDirect(conn, meta, buf) => FeatureWorkerOutput::SendDirect(conn, meta, buf),
            FeatureWorkerOutput::SendRoute(route, meta, buf) => FeatureWorkerOutput::SendRoute(route, meta, buf),
            FeatureWorkerOutput::RawDirect(conn, buf) => FeatureWorkerOutput::RawDirect(conn, buf),
            FeatureWorkerOutput::RawBroadcast(conns, buf) => FeatureWorkerOutput::RawBroadcast(conns, buf),
            FeatureWorkerOutput::RawDirect2(conn, buf) => FeatureWorkerOutput::RawDirect2(conn, buf),
            FeatureWorkerOutput::RawBroadcast2(conns, buf) => FeatureWorkerOutput::RawBroadcast2(conns, buf),
            #[cfg(feature = "vpn")]
            FeatureWorkerOutput::TunPkt(buf) => FeatureWorkerOutput::TunPkt(buf),
        }
    }
}

pub struct FeatureWorkerContext {
    pub node_id: NodeId,
    pub router: ShadowRouter<NetPair>,
}

pub trait FeatureWorker<UserData, SdkControl, SdkEvent, ToController, ToWorker>: TaskSwitcherChild<FeatureWorkerOutput<UserData, SdkControl, SdkEvent, ToController>> {
    fn on_tick(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64, _tick_count: u64) {}
    fn on_network_raw(&mut self, ctx: &mut FeatureWorkerContext, now: u64, conn: ConnId, _pair: NetPair, header: TransportMsgHeader, mut buf: Buffer) {
        let header_len = header.serialize_size();
        buf.move_front_right(header_len).expect("Buffer should bigger or equal header");
        self.on_input(ctx, now, FeatureWorkerInput::Network(conn, (&header).into(), buf));
    }
    fn on_input(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<UserData, SdkControl, ToWorker>);
}
