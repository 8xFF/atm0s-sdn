pub mod data;
pub mod dht_kv;
pub mod lazy_kv;
pub mod neighbours;
pub mod router_sync;
pub mod vpn;

///
/// FeatureManager need wrap child features in a struct to manage them
/// This is a helper struct to help FeatureManager to manage the features
///

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Features {
    Neighbours = neighbours::FEATURE_ID,
    Data = data::FEATURE_ID,
    RouterSync = router_sync::FEATURE_ID,
    Vpn = vpn::FEATURE_ID,
    DhtKv = dht_kv::FEATURE_ID,
}

impl TryFrom<u8> for Features {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            neighbours::FEATURE_ID => Ok(Features::Neighbours),
            data::FEATURE_ID => Ok(Features::Data),
            router_sync::FEATURE_ID => Ok(Features::RouterSync),
            vpn::FEATURE_ID => Ok(Features::Vpn),
            dht_kv::FEATURE_ID => Ok(Features::DhtKv),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum FeaturesControl {
    Neighbours(neighbours::Control),
    Data(data::Control),
    RouterSync(router_sync::Control),
    Vpn(vpn::Control),
    DhtKv(dht_kv::Control),
}

impl FeaturesControl {
    pub fn to_feature(&self) -> Features {
        match self {
            Self::Neighbours(_) => Features::Neighbours,
            Self::Data(_) => Features::Data,
            Self::RouterSync(_) => Features::RouterSync,
            Self::Vpn(_) => Features::Vpn,
            Self::DhtKv(_) => Features::DhtKv,
        }
    }
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum FeaturesEvent {
    Neighbours(neighbours::Event),
    Data(data::Event),
    RouterSync(router_sync::Event),
    Vpn(vpn::Event),
    DhtKv(dht_kv::Event),
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum FeaturesToController {
    Neighbours(neighbours::ToController),
    Data(data::ToController),
    RouterSync(router_sync::ToController),
    Vpn(vpn::ToController),
    DhtKv(dht_kv::ToController),
}

impl FeaturesToController {
    pub fn to_feature(&self) -> Features {
        match self {
            Self::Neighbours(_) => Features::Neighbours,
            Self::Data(_) => Features::Data,
            Self::RouterSync(_) => Features::RouterSync,
            Self::Vpn(_) => Features::Vpn,
            Self::DhtKv(_) => Features::DhtKv,
        }
    }
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum FeaturesToWorker {
    Neighbours(neighbours::ToWorker),
    Data(data::ToWorker),
    RouterSync(router_sync::ToWorker),
    Vpn(vpn::ToWorker),
    DhtKv(dht_kv::ToWorker),
}

impl FeaturesToWorker {
    pub fn to_feature(&self) -> Features {
        match self {
            Self::Neighbours(_) => Features::Neighbours,
            Self::Data(_) => Features::Data,
            Self::RouterSync(_) => Features::RouterSync,
            Self::Vpn(_) => Features::Vpn,
            Self::DhtKv(_) => Features::DhtKv,
        }
    }
}
