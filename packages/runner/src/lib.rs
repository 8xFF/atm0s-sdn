pub use atm0s_sdn_identity::{ConnDirection, ConnId, NodeAddr, NodeAddrBuilder, NodeId, NodeIdType, Protocol};
pub use atm0s_sdn_network::controller_plane::ControllerPlaneCfg;
pub use atm0s_sdn_network::convert_enum;
pub use atm0s_sdn_network::data_plane::DataPlaneCfg;
use atm0s_sdn_network::features::FeaturesControl;
pub use atm0s_sdn_network::{
    base, features, secure, services,
    worker::{SdnWorker, SdnWorkerBusEvent, SdnWorkerCfg, SdnWorkerInput, SdnWorkerOutput},
};
pub use atm0s_sdn_network::{
    base::ServiceId,
    data_plane::{NetInput, NetOutput},
};
pub use atm0s_sdn_router::{shadow::ShadowRouterHistory, ServiceBroadcastLevel};
pub use sans_io_runtime;

mod builder;
mod history;
mod time;
mod worker_inner;

pub use builder::SdnBuilder;
pub use history::DataWorkerHistory;
pub use time::{TimePivot, TimeTicker};
pub use worker_inner::{SdnChannel, SdnController, SdnEvent, SdnExtIn, SdnExtOut, SdnOwner};

pub trait SdnControllerUtils<SC> {
    fn connect_to(&mut self, addr: NodeAddr);
    fn feature_control(&mut self, cmd: FeaturesControl);
    fn service_control(&mut self, service: ServiceId, cmd: SC);
}

impl<SC: 'static + Send + Sync + Clone, SE: 'static + Send + Sync + Clone, TC: 'static + Send + Sync + Clone, TW: 'static + Send + Sync + Clone> SdnControllerUtils<SC>
    for SdnController<SC, SE, TC, TW>
{
    fn connect_to(&mut self, addr: NodeAddr) {
        self.send_to(0, SdnExtIn::ConnectTo(addr));
    }
    fn feature_control(&mut self, cmd: FeaturesControl) {
        self.send_to(0, SdnExtIn::FeaturesControl(cmd));
    }

    fn service_control(&mut self, service: ServiceId, cmd: SC) {
        self.send_to(0, SdnExtIn::ServicesControl(service, cmd));
    }
}
