use serde::{Serialize, Deserialize};

use crate::{NodeAliasId, sdk::{NodeAliasError, NodeAliasResult}};

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum DirectMsg {
    Query(NodeAliasId),
    Response {
        alias: NodeAliasId,
        added_at: Option<u64>,
    },
}

#[derive(Serialize, Deserialize, Debug)]
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