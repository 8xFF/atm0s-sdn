use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_router::{shadow::ShadowRouter, RouteRule};

use super::{ConnectionCtx, ConnectionEvent, GenericBuffer, ServiceId};

///
///

#[derive(Debug, Clone)]
pub enum FeatureSharedInput {
    Tick(u64),
    Connection(ConnectionEvent),
}

#[derive(Debug, Clone)]
pub enum FeatureInput<'a, Control, ToController> {
    FromWorker(ToController),
    Control(ServiceId, Control),
    ForwardNetFromWorker(&'a ConnectionCtx, Vec<u8>),
    ForwardLocalFromWorker(Vec<u8>),
}

#[derive(Debug)]
pub enum FeatureOutput<Event, ToWorker> {
    BroadcastToWorkers(ToWorker),
    Event(ServiceId, Event),
    SendDirect(ConnId, Vec<u8>),
    SendRoute(RouteRule, Vec<u8>),
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
            FeatureOutput::BroadcastToWorkers(to) => FeatureOutput::BroadcastToWorkers(to.into()),
            FeatureOutput::Event(service, event) => FeatureOutput::Event(service, event.into()),
            FeatureOutput::SendDirect(conn, msg) => FeatureOutput::SendDirect(conn, msg),
            FeatureOutput::SendRoute(rule, buf) => FeatureOutput::SendRoute(rule, buf),
            FeatureOutput::NeighboursConnectTo(addr) => FeatureOutput::NeighboursConnectTo(addr),
            FeatureOutput::NeighboursDisconnectFrom(id) => FeatureOutput::NeighboursDisconnectFrom(id),
        }
    }
}

pub trait Feature<Control, Event, ToController, ToWorker>: Send + Sync {
    fn feature_type(&self) -> u8;
    fn feature_name(&self) -> &str;
    fn on_shared_input(&mut self, _now: u64, _input: FeatureSharedInput);
    fn on_input<'a>(&mut self, now_ms: u64, input: FeatureInput<'a, Control, ToController>);
    fn pop_output(&mut self) -> Option<FeatureOutput<Event, ToWorker>>;
}

///
///

pub enum FeatureWorkerInput<Control, ToWorker> {
    FromController(ToWorker),
    Control(ServiceId, Control),
    Network(ConnId, Vec<u8>),
    Local(Vec<u8>),
}

pub enum FeatureWorkerOutput<Control, Event, ToController> {
    ForwardControlToController(ServiceId, Control),
    ForwardNetworkToController(ConnId, Vec<u8>),
    ForwardLocalToController(Vec<u8>),
    ToController(ToController),
    Event(ServiceId, Event),
    SendDirect(ConnId, Vec<u8>),
    SendRoute(RouteRule, Vec<u8>),
}

impl<Control, Event, ToController> FeatureWorkerOutput<Control, Event, ToController> {
    pub fn into2<Control2, Event2, ToController2>(self) -> FeatureWorkerOutput<Control2, Event2, ToController2>
    where
        Control2: From<Control>,
        Event2: From<Event>,
        ToController2: From<ToController>,
    {
        match self {
            FeatureWorkerOutput::ForwardControlToController(service, control) => FeatureWorkerOutput::ForwardControlToController(service, control.into()),
            FeatureWorkerOutput::ForwardNetworkToController(conn, msg) => FeatureWorkerOutput::ForwardNetworkToController(conn, msg),
            FeatureWorkerOutput::ForwardLocalToController(buf) => FeatureWorkerOutput::ForwardLocalToController(buf),
            FeatureWorkerOutput::ToController(to) => FeatureWorkerOutput::ToController(to.into()),
            FeatureWorkerOutput::Event(service, event) => FeatureWorkerOutput::Event(service, event.into()),
            FeatureWorkerOutput::SendDirect(conn, buf) => FeatureWorkerOutput::SendDirect(conn, buf),
            FeatureWorkerOutput::SendRoute(route, buf) => FeatureWorkerOutput::SendRoute(route, buf),
        }
    }
}

pub struct FeatureWorkerContext {
    pub router: ShadowRouter<SocketAddr>,
}

pub trait FeatureWorker<SdkControl, SdkEvent, ToController, ToWorker> {
    fn feature_type(&self) -> u8;
    fn feature_name(&self) -> &str;
    fn on_tick(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64) {}
    fn on_network_raw<'a>(
        &mut self,
        ctx: &mut FeatureWorkerContext,
        now: u64,
        conn: ConnId,
        header_len: usize,
        buf: GenericBuffer<'a>,
    ) -> Option<FeatureWorkerOutput<SdkControl, SdkEvent, ToController>> {
        let buf = &(&buf)[header_len..];
        self.on_input(ctx, now, FeatureWorkerInput::Network(conn, buf.to_vec()))
    }
    fn on_input<'a>(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<SdkControl, ToWorker>) -> Option<FeatureWorkerOutput<SdkControl, SdkEvent, ToController>> {
        match input {
            FeatureWorkerInput::Control(service, control) => Some(FeatureWorkerOutput::ForwardControlToController(service, control)),
            FeatureWorkerInput::Network(conn, buf) => Some(FeatureWorkerOutput::ForwardNetworkToController(conn, buf)),
            FeatureWorkerInput::FromController(_event) => {
                log::warn!("No handler for FromController in {}", self.feature_name());
                None
            }
            FeatureWorkerInput::Local(_msg) => {
                log::warn!("No handler for local message in {}", self.feature_name());
                None
            }
        }
    }
    fn pop_output(&mut self) -> Option<FeatureWorkerOutput<SdkControl, SdkEvent, ToController>> {
        None
    }
}
