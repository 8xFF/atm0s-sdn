use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_router::shadow::ShadowRouter;

use super::{ConnectionCtx, ConnectionStats, ServiceId, TransportMsg};

///
///

#[derive(Clone)]
pub enum FeatureSharedInput<'a> {
    Tick(u64),
    ConnectionConnected(&'a ConnectionCtx),
    ConnectionStats(&'a ConnectionCtx, &'a ConnectionStats),
    ConnectionData(&'a ConnectionCtx, TransportMsg),
    ConnectionDisconnected(&'a ConnectionCtx),
}

pub enum FeatureInput<'a, Control, ToController> {
    Shared(FeatureSharedInput<'a>),
    FromWorker(ToController),
    Control(ServiceId, Control),
}

pub enum FeatureOutput<Event, ToWorker> {
    BroadcastToWorkers(ToWorker),
    Event(ServiceId, Event),
    SendDirect(ConnId, TransportMsg),
    SendRoute(TransportMsg),
    NeighboursConnectTo(NodeAddr),
    NeighboursDisconnectFrom(NodeId),
}

impl<Event, ToWorker> FeatureOutput<Event, ToWorker> {
    pub fn into2<Event2, ToWorker2>(self) -> FeatureOutput<Event2, ToWorker2>
    where
        Event2: From<Event>,
        ToWorker2: From<ToWorker>,
    {
        match self {
            FeatureOutput::BroadcastToWorkers(to) => FeatureOutput::BroadcastToWorkers(to.into()),
            FeatureOutput::Event(service, event) => FeatureOutput::Event(service, event.into()),
            FeatureOutput::SendDirect(conn, msg) => FeatureOutput::SendDirect(conn, msg),
            FeatureOutput::SendRoute(msg) => FeatureOutput::SendRoute(msg),
            FeatureOutput::NeighboursConnectTo(addr) => FeatureOutput::NeighboursConnectTo(addr),
            FeatureOutput::NeighboursDisconnectFrom(id) => FeatureOutput::NeighboursDisconnectFrom(id),
        }
    }
}

pub trait Feature<Control, Event, ToController, ToWorker>: Send + Sync {
    fn feature_type(&self) -> u8;
    fn feature_name(&self) -> &str;
    fn on_input<'a>(&mut self, now_ms: u64, input: FeatureInput<'a, Control, ToController>);
    fn pop_output(&mut self) -> Option<FeatureOutput<Event, ToWorker>>;
}

///
///

pub enum FeatureWorkerInput<Control, ToWorker> {
    FromController(ToWorker),
    Control(ServiceId, Control),
    Network(ConnId, TransportMsg),
}

pub enum FeatureWorkerOutput<Control, Event, ToController> {
    ForwardControlToController(ServiceId, Control),
    ForwardNetworkToController(ConnId, TransportMsg),
    ToController(ToController),
    Event(ServiceId, Event),
    SendDirect(ConnId, TransportMsg),
    SendRoute(TransportMsg),
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
            FeatureWorkerOutput::ToController(to) => FeatureWorkerOutput::ToController(to.into()),
            FeatureWorkerOutput::Event(service, event) => FeatureWorkerOutput::Event(service, event.into()),
            FeatureWorkerOutput::SendDirect(conn, msg) => FeatureWorkerOutput::SendDirect(conn, msg),
            FeatureWorkerOutput::SendRoute(msg) => FeatureWorkerOutput::SendRoute(msg),
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
    fn on_input(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<SdkControl, ToWorker>) -> Option<FeatureWorkerOutput<SdkControl, SdkEvent, ToController>>;
    fn pop_output(&mut self) -> Option<FeatureWorkerOutput<SdkControl, SdkEvent, ToController>>;
}
