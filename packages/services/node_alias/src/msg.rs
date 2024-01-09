use serde::{Deserialize, Serialize};

use crate::{
    sdk::{NodeAliasError, NodeAliasResult},
    NodeAliasId,
};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) enum DirectMsg {
    Query(NodeAliasId),
    Response { alias: NodeAliasId, added_at: Option<u64> },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) enum BroadcastMsg {
    Register(NodeAliasId),
    Unregister(NodeAliasId),
    Query(NodeAliasId),
}

pub(crate) enum SdkControl {
    Register(NodeAliasId),
    Unregister(NodeAliasId),
    Query(NodeAliasId, Box<dyn FnOnce(Result<NodeAliasResult, NodeAliasError>) + Send>),
}
