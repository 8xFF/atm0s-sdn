pub mod alias;
pub mod data;
pub mod dht_kv;
pub mod neighbours;
pub mod pubsub;
pub mod router_sync;
pub mod socket;
pub mod vpn;

///
/// FeatureManager need wrap child features in a struct to manage them
/// This is a helper struct to help FeatureManager to manage the features
///

#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::TryFromPrimitive, num_enum::IntoPrimitive)]
#[repr(u8)]
pub enum Features {
    Neighbours = neighbours::FEATURE_ID,
    Data = data::FEATURE_ID,
    RouterSync = router_sync::FEATURE_ID,
    Vpn = vpn::FEATURE_ID,
    DhtKv = dht_kv::FEATURE_ID,
    PubSub = pubsub::FEATURE_ID,
    Alias = alias::FEATURE_ID,
    Socket = socket::FEATURE_ID,
}

#[derive(Debug, Clone, PartialEq, Eq, convert_enum::From)]
pub enum FeaturesControl {
    Neighbours(neighbours::Control),
    Data(data::Control),
    RouterSync(router_sync::Control),
    Vpn(vpn::Control),
    DhtKv(dht_kv::Control),
    PubSub(pubsub::Control),
    Alias(alias::Control),
    Socket(socket::Control),
}

impl FeaturesControl {
    pub fn to_feature(&self) -> Features {
        match self {
            Self::Neighbours(_) => Features::Neighbours,
            Self::Data(_) => Features::Data,
            Self::RouterSync(_) => Features::RouterSync,
            Self::Vpn(_) => Features::Vpn,
            Self::DhtKv(_) => Features::DhtKv,
            Self::PubSub(_) => Features::PubSub,
            Self::Alias(_) => Features::Alias,
            Self::Socket(_) => Features::Socket,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, convert_enum::From)]
pub enum FeaturesEvent {
    Neighbours(neighbours::Event),
    Data(data::Event),
    RouterSync(router_sync::Event),
    Vpn(vpn::Event),
    DhtKv(dht_kv::Event),
    PubSub(pubsub::Event),
    Alias(alias::Event),
    Socket(socket::Event),
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum FeaturesToController {
    Neighbours(neighbours::ToController),
    Data(data::ToController),
    RouterSync(router_sync::ToController),
    Vpn(vpn::ToController),
    DhtKv(dht_kv::ToController),
    PubSub(pubsub::ToController),
    Alias(alias::ToController),
    Socket(socket::ToController),
}

impl FeaturesToController {
    pub fn to_feature(&self) -> Features {
        match self {
            Self::Neighbours(_) => Features::Neighbours,
            Self::Data(_) => Features::Data,
            Self::RouterSync(_) => Features::RouterSync,
            Self::Vpn(_) => Features::Vpn,
            Self::DhtKv(_) => Features::DhtKv,
            Self::PubSub(_) => Features::PubSub,
            Self::Alias(_) => Features::Alias,
            Self::Socket(_) => Features::Socket,
        }
    }
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum FeaturesToWorker<UserData> {
    Neighbours(neighbours::ToWorker),
    Data(data::ToWorker),
    RouterSync(router_sync::ToWorker),
    Vpn(vpn::ToWorker),
    DhtKv(dht_kv::ToWorker),
    PubSub(pubsub::ToWorker<UserData>),
    Alias(alias::ToWorker),
    Socket(socket::ToWorker<UserData>),
}

impl<UserData> FeaturesToWorker<UserData> {
    pub fn to_feature(&self) -> Features {
        match self {
            Self::Neighbours(_) => Features::Neighbours,
            Self::Data(_) => Features::Data,
            Self::RouterSync(_) => Features::RouterSync,
            Self::Vpn(_) => Features::Vpn,
            Self::DhtKv(_) => Features::DhtKv,
            Self::PubSub(_) => Features::PubSub,
            Self::Alias(_) => Features::Alias,
            Self::Socket(_) => Features::Socket,
        }
    }
}
