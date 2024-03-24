use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_router::{shadow::ShadowRouter, RouteRule};

use super::{ConnectionCtx, ConnectionEvent, GenericBuffer, GenericBufferMut, ServiceId, Ttl};

///
///

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum FeatureControlActor {
    Controller,
    Service(ServiceId),
}

#[derive(Debug, Clone)]
pub enum FeatureSharedInput {
    Tick(u64),
    Connection(ConnectionEvent),
}

#[derive(Debug, Clone)]
pub enum FeatureInput<'a, Control, ToController> {
    FromWorker(ToController),
    Control(FeatureControlActor, Control),
    Net(&'a ConnectionCtx, Vec<u8>),
    Local(Vec<u8>),
}

#[derive(Debug)]
pub enum FeatureOutput<Event, ToWorker> {
    /// First bool is flag for broadcast or not
    ToWorker(bool, ToWorker),
    Event(FeatureControlActor, Event),
    SendDirect(ConnId, Vec<u8>),
    SendRoute(RouteRule, Ttl, Vec<u8>),
    NeighboursConnectTo(NodeAddr),
    NeighboursDisconnectFrom(NodeId),
}

impl<'a, Event, ToWorker> FeatureOutput<Event, ToWorker> {
    pub fn into2<Event2, ToWorker2>(self) -> FeatureOutput<Event2, ToWorker2>
    where
        Event2: From<Event>,
        ToWorker2: From<ToWorker>,
    {
        match self {
            FeatureOutput::ToWorker(is_broadcast, to) => FeatureOutput::ToWorker(is_broadcast, to.into()),
            FeatureOutput::Event(actor, event) => FeatureOutput::Event(actor, event.into()),
            FeatureOutput::SendDirect(conn, msg) => FeatureOutput::SendDirect(conn, msg),
            FeatureOutput::SendRoute(rule, ttl, buf) => FeatureOutput::SendRoute(rule, ttl, buf),
            FeatureOutput::NeighboursConnectTo(addr) => FeatureOutput::NeighboursConnectTo(addr),
            FeatureOutput::NeighboursDisconnectFrom(id) => FeatureOutput::NeighboursDisconnectFrom(id),
        }
    }
}

pub trait Feature<Control, Event, ToController, ToWorker> {
    fn feature_type(&self) -> u8;
    fn feature_name(&self) -> &str;
    fn on_shared_input(&mut self, _now: u64, _input: FeatureSharedInput);
    fn on_input<'a>(&mut self, now_ms: u64, input: FeatureInput<'a, Control, ToController>);
    fn pop_output(&mut self) -> Option<FeatureOutput<Event, ToWorker>>;
}

///
///

pub enum FeatureWorkerInput<'a, Control, ToWorker> {
    /// First bool is flag for broadcast or not
    FromController(bool, ToWorker),
    Control(FeatureControlActor, Control),
    Network(ConnId, GenericBuffer<'a>),
    Local(GenericBuffer<'a>),
    TunPkt(GenericBufferMut<'a>),
}

#[derive(Clone)]
pub enum FeatureWorkerOutput<'a, Control, Event, ToController> {
    ForwardControlToController(FeatureControlActor, Control),
    ForwardNetworkToController(ConnId, Vec<u8>),
    ForwardLocalToController(Vec<u8>),
    ToController(ToController),
    Event(FeatureControlActor, Event),
    SendDirect(ConnId, Vec<u8>),
    SendRoute(RouteRule, Ttl, Vec<u8>),
    RawDirect(ConnId, GenericBuffer<'a>),
    RawBroadcast(Vec<ConnId>, GenericBuffer<'a>),
    RawDirect2(SocketAddr, GenericBuffer<'a>),
    RawBroadcast2(Vec<SocketAddr>, GenericBuffer<'a>),
    TunPkt(GenericBuffer<'a>),
}

impl<'a, Control, Event, ToController> FeatureWorkerOutput<'a, Control, Event, ToController> {
    pub fn into2<Control2, Event2, ToController2>(self) -> FeatureWorkerOutput<'a, Control2, Event2, ToController2>
    where
        Control2: From<Control>,
        Event2: From<Event>,
        ToController2: From<ToController>,
    {
        match self {
            FeatureWorkerOutput::ForwardControlToController(actor, control) => FeatureWorkerOutput::ForwardControlToController(actor, control.into()),
            FeatureWorkerOutput::ForwardNetworkToController(conn, msg) => FeatureWorkerOutput::ForwardNetworkToController(conn, msg),
            FeatureWorkerOutput::ForwardLocalToController(buf) => FeatureWorkerOutput::ForwardLocalToController(buf),
            FeatureWorkerOutput::ToController(to) => FeatureWorkerOutput::ToController(to.into()),
            FeatureWorkerOutput::Event(actor, event) => FeatureWorkerOutput::Event(actor, event.into()),
            FeatureWorkerOutput::SendDirect(conn, buf) => FeatureWorkerOutput::SendDirect(conn, buf),
            FeatureWorkerOutput::SendRoute(route, ttl, buf) => FeatureWorkerOutput::SendRoute(route, ttl, buf),
            FeatureWorkerOutput::RawDirect(conn, buf) => FeatureWorkerOutput::RawDirect(conn, buf),
            FeatureWorkerOutput::RawBroadcast(conns, buf) => FeatureWorkerOutput::RawBroadcast(conns, buf),
            FeatureWorkerOutput::RawDirect2(conn, buf) => FeatureWorkerOutput::RawDirect2(conn, buf),
            FeatureWorkerOutput::RawBroadcast2(conns, buf) => FeatureWorkerOutput::RawBroadcast2(conns, buf),
            FeatureWorkerOutput::TunPkt(buf) => FeatureWorkerOutput::TunPkt(buf),
        }
    }

    pub fn owned(self) -> FeatureWorkerOutput<'static, Control, Event, ToController> {
        match self {
            FeatureWorkerOutput::ForwardControlToController(actor, control) => FeatureWorkerOutput::ForwardControlToController(actor, control),
            FeatureWorkerOutput::ForwardNetworkToController(conn, msg) => FeatureWorkerOutput::ForwardNetworkToController(conn, msg),
            FeatureWorkerOutput::ForwardLocalToController(buf) => FeatureWorkerOutput::ForwardLocalToController(buf),
            FeatureWorkerOutput::ToController(to) => FeatureWorkerOutput::ToController(to),
            FeatureWorkerOutput::Event(actor, event) => FeatureWorkerOutput::Event(actor, event),
            FeatureWorkerOutput::SendDirect(conn, buf) => FeatureWorkerOutput::SendDirect(conn, buf),
            FeatureWorkerOutput::SendRoute(route, ttl, buf) => FeatureWorkerOutput::SendRoute(route, ttl, buf),
            FeatureWorkerOutput::RawDirect(conn, buf) => FeatureWorkerOutput::RawDirect(conn, buf.owned()),
            FeatureWorkerOutput::RawBroadcast(conns, buf) => FeatureWorkerOutput::RawBroadcast(conns, buf.owned()),
            FeatureWorkerOutput::RawDirect2(conn, buf) => FeatureWorkerOutput::RawDirect2(conn, buf.owned()),
            FeatureWorkerOutput::RawBroadcast2(conns, buf) => FeatureWorkerOutput::RawBroadcast2(conns, buf.owned()),
            FeatureWorkerOutput::TunPkt(buf) => FeatureWorkerOutput::TunPkt(buf.owned()),
        }
    }
}

pub struct FeatureWorkerContext {
    pub node_id: NodeId,
    pub router: ShadowRouter<SocketAddr>,
}

pub trait FeatureWorker<SdkControl, SdkEvent, ToController, ToWorker> {
    fn feature_type(&self) -> u8;
    fn feature_name(&self) -> &str;
    fn on_tick(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64, _tick_count: u64) {}
    fn on_network_raw<'a>(
        &mut self,
        ctx: &mut FeatureWorkerContext,
        now: u64,
        conn: ConnId,
        _remote: SocketAddr,
        header_len: usize,
        buf: GenericBuffer<'a>,
    ) -> Option<FeatureWorkerOutput<'a, SdkControl, SdkEvent, ToController>> {
        self.on_input(ctx, now, FeatureWorkerInput::Network(conn, buf.sub_view(header_len..buf.len())))
    }
    fn on_input<'a>(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<'a, SdkControl, ToWorker>) -> Option<FeatureWorkerOutput<'a, SdkControl, SdkEvent, ToController>> {
        match input {
            FeatureWorkerInput::Control(actor, control) => Some(FeatureWorkerOutput::ForwardControlToController(actor, control)),
            FeatureWorkerInput::Network(conn, buf) => Some(FeatureWorkerOutput::ForwardNetworkToController(conn, buf.to_vec())),
            FeatureWorkerInput::TunPkt(_buf) => None,
            FeatureWorkerInput::FromController(_, _) => {
                log::warn!("No handler for FromController in {}", self.feature_name());
                None
            }
            FeatureWorkerInput::Local(buf) => Some(FeatureWorkerOutput::ForwardLocalToController(buf.to_vec())),
        }
    }
    fn pop_output<'a>(&mut self) -> Option<FeatureWorkerOutput<'a, SdkControl, SdkEvent, ToController>> {
        None
    }
}
