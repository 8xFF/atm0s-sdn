pub mod data;
pub mod neighbours;

///
/// FeatureManager need wrap child features in a struct to manage them
/// This is a helper struct to help FeatureManager to manage the features
///

#[derive(convert_enum::From)]
pub enum FeaturesControl {
    Neighbours(neighbours::Control),
    Data(data::Control),
}

#[derive(convert_enum::From)]
pub enum FeaturesEvent {
    Neighbours(neighbours::Event),
    Data(data::Event),
}

#[derive(convert_enum::From)]
pub enum FeaturesToController {
    Neighbours(neighbours::ToController),
    Data(data::ToController),
}

#[derive(convert_enum::From)]
pub enum FeaturesToWorker {
    Neighbours(neighbours::ToWorker),
    Data(data::ToWorker),
}
