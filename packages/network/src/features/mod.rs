pub mod data;
pub mod neighbours;
pub mod router_sync;
pub mod vpn;

///
/// FeatureManager need wrap child features in a struct to manage them
/// This is a helper struct to help FeatureManager to manage the features
///

#[derive(Debug, Clone, convert_enum::From)]
pub enum FeaturesControl {
    Neighbours(neighbours::Control),
    Data(data::Control),
    RouterSync(router_sync::Control),
    Vpn(vpn::Control),
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum FeaturesEvent {
    Neighbours(neighbours::Event),
    Data(data::Event),
    RouterSync(router_sync::Event),
    Vpn(vpn::Event),
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum FeaturesToController {
    Neighbours(neighbours::ToController),
    Data(data::ToController),
    RouterSync(router_sync::ToController),
    Vpn(vpn::ToController),
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum FeaturesToWorker {
    Neighbours(neighbours::ToWorker),
    Data(data::ToWorker),
    RouterSync(router_sync::ToWorker),
    Vpn(vpn::ToWorker),
}
