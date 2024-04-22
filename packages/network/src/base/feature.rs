use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_router::{shadow::ShadowRouter, RouteRule};

#[cfg(feature = "vpn")]
use super::BufferMut;
use super::{Buffer, ConnectionCtx, ConnectionEvent, ServiceId, TransportMsgHeader, Ttl};

///
///

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
    Net(&'a ConnectionCtx, NetIncomingMeta, Vec<u8>),
    Local(NetIncomingMeta, Vec<u8>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum FeatureOutput<UserData, Event, ToWorker> {
    /// First bool is flag for broadcast or not
    ToWorker(bool, ToWorker),
    Event(FeatureControlActor<UserData>, Event),
    SendDirect(ConnId, NetOutgoingMeta, Vec<u8>),
    SendRoute(RouteRule, NetOutgoingMeta, Vec<u8>),
    NeighboursConnectTo(NodeAddr),
    NeighboursDisconnectFrom(NodeId),
}

impl<'a, UserData, Event, ToWorker> FeatureOutput<UserData, Event, ToWorker> {
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

pub trait Feature<UserData, Control, Event, ToController, ToWorker> {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, _now: u64, _input: FeatureSharedInput);
    fn on_input<'a>(&mut self, _ctx: &FeatureContext, now_ms: u64, input: FeatureInput<'a, UserData, Control, ToController>);
    fn pop_output(&mut self, _ctx: &FeatureContext) -> Option<FeatureOutput<UserData, Event, ToWorker>>;
}

///
///

pub enum FeatureWorkerInput<'a, UserData, Control, ToWorker> {
    /// First bool is flag for broadcast or not
    FromController(bool, ToWorker),
    Control(FeatureControlActor<UserData>, Control),
    Network(ConnId, NetIncomingMeta, Buffer<'a>),
    Local(NetIncomingMeta, Buffer<'a>),
    #[cfg(feature = "vpn")]
    TunPkt(BufferMut<'a>),
}

#[derive(Clone)]
pub enum FeatureWorkerOutput<'a, UserData, Control, Event, ToController> {
    ForwardControlToController(FeatureControlActor<UserData>, Control),
    ForwardNetworkToController(ConnId, NetIncomingMeta, Vec<u8>),
    ForwardLocalToController(NetIncomingMeta, Vec<u8>),
    ToController(ToController),
    Event(FeatureControlActor<UserData>, Event),
    SendDirect(ConnId, NetOutgoingMeta, Vec<u8>),
    SendRoute(RouteRule, NetOutgoingMeta, Vec<u8>),
    RawDirect(ConnId, Buffer<'a>),
    RawBroadcast(Vec<ConnId>, Buffer<'a>),
    RawDirect2(SocketAddr, Buffer<'a>),
    RawBroadcast2(Vec<SocketAddr>, Buffer<'a>),
    #[cfg(feature = "vpn")]
    TunPkt(Buffer<'a>),
}

impl<'a, UserData, Control, Event, ToController> FeatureWorkerOutput<'a, UserData, Control, Event, ToController> {
    pub fn into2<UserData2, Control2, Event2, ToController2>(self) -> FeatureWorkerOutput<'a, UserData2, Control2, Event2, ToController2>
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

    pub fn owned(self) -> FeatureWorkerOutput<'static, UserData, Control, Event, ToController> {
        match self {
            FeatureWorkerOutput::ForwardControlToController(actor, control) => FeatureWorkerOutput::ForwardControlToController(actor, control),
            FeatureWorkerOutput::ForwardNetworkToController(conn, header, msg) => FeatureWorkerOutput::ForwardNetworkToController(conn, header, msg),
            FeatureWorkerOutput::ForwardLocalToController(header, buf) => FeatureWorkerOutput::ForwardLocalToController(header, buf),
            FeatureWorkerOutput::ToController(to) => FeatureWorkerOutput::ToController(to),
            FeatureWorkerOutput::Event(actor, event) => FeatureWorkerOutput::Event(actor, event),
            FeatureWorkerOutput::SendDirect(conn, meta, buf) => FeatureWorkerOutput::SendDirect(conn, meta, buf),
            FeatureWorkerOutput::SendRoute(route, ttl, buf) => FeatureWorkerOutput::SendRoute(route, ttl, buf),
            FeatureWorkerOutput::RawDirect(conn, buf) => FeatureWorkerOutput::RawDirect(conn, buf.owned()),
            FeatureWorkerOutput::RawBroadcast(conns, buf) => FeatureWorkerOutput::RawBroadcast(conns, buf.owned()),
            FeatureWorkerOutput::RawDirect2(conn, buf) => FeatureWorkerOutput::RawDirect2(conn, buf.owned()),
            FeatureWorkerOutput::RawBroadcast2(conns, buf) => FeatureWorkerOutput::RawBroadcast2(conns, buf.owned()),
            #[cfg(feature = "vpn")]
            FeatureWorkerOutput::TunPkt(buf) => FeatureWorkerOutput::TunPkt(buf.owned()),
        }
    }
}

pub struct FeatureWorkerContext {
    pub node_id: NodeId,
    pub router: ShadowRouter<SocketAddr>,
}

pub trait FeatureWorker<UserData, SdkControl, SdkEvent, ToController, ToWorker> {
    fn on_tick(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64, _tick_count: u64) {}
    fn on_network_raw<'a>(
        &mut self,
        ctx: &mut FeatureWorkerContext,
        now: u64,
        conn: ConnId,
        _remote: SocketAddr,
        header: TransportMsgHeader,
        mut buf: Buffer<'a>,
    ) -> Option<FeatureWorkerOutput<'a, UserData, SdkControl, SdkEvent, ToController>> {
        let header_len = header.serialize_size();
        buf.pop_front(header_len).expect("Buffer should bigger or equal header");
        self.on_input(ctx, now, FeatureWorkerInput::Network(conn, (&header).into(), buf))
    }
    fn on_input<'a>(
        &mut self,
        _ctx: &mut FeatureWorkerContext,
        _now: u64,
        input: FeatureWorkerInput<'a, UserData, SdkControl, ToWorker>,
    ) -> Option<FeatureWorkerOutput<'a, UserData, SdkControl, SdkEvent, ToController>> {
        match input {
            FeatureWorkerInput::Control(actor, control) => Some(FeatureWorkerOutput::ForwardControlToController(actor, control)),
            FeatureWorkerInput::Network(conn, header, buf) => Some(FeatureWorkerOutput::ForwardNetworkToController(conn, header, buf.to_vec())),
            #[cfg(feature = "vpn")]
            FeatureWorkerInput::TunPkt(_buf) => None,
            FeatureWorkerInput::FromController(_, _) => {
                log::warn!("No handler for FromController");
                None
            }
            FeatureWorkerInput::Local(header, buf) => Some(FeatureWorkerOutput::ForwardLocalToController(header, buf.to_vec())),
        }
    }
    fn pop_output<'a>(&mut self, _ctx: &mut FeatureWorkerContext) -> Option<FeatureWorkerOutput<'a, UserData, SdkControl, SdkEvent, ToController>> {
        None
    }
}
