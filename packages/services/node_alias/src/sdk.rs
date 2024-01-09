use std::{sync::Arc, collections::VecDeque};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_utils::awaker::Awaker;
use parking_lot::Mutex;

use crate::{msg::SdkControl, NodeAliasId};

#[derive(Debug, PartialEq)]
pub enum NodeAliasResult {
    FromLocal,
    FromHint(NodeId),
    FromScan(NodeId),
}

#[derive(Debug, PartialEq)]
pub enum NodeAliasError {
    Timeout,
}

#[derive(Clone, Default)]
pub struct NodeAliasSdk {
    sdk_control_queue: Arc<Mutex<VecDeque<SdkControl>>>,
    awaker: Arc<Mutex<Option<Arc<dyn Awaker>>>>,
}

impl NodeAliasSdk {
    pub(crate) fn set_awaker(&self, awaker: Arc<dyn Awaker>) {
        *self.awaker.lock() = Some(awaker);
    }
    
    pub fn register(&self, alias: NodeAliasId) {
        log::info!("[NodeAliasSdk] Register alias: {}", alias);
        self.sdk_control_queue.lock().push_back(SdkControl::Register(alias));
        if let Some(awaker) = &*self.awaker.lock() {
            awaker.notify();
        }
    }

    pub fn unregister(&self, alias: NodeAliasId) {
        log::info!("[NodeAliasSdk] Unregister alias: {}", alias);
        self.sdk_control_queue.lock().push_back(SdkControl::Unregister(alias));
        if let Some(awaker) = &*self.awaker.lock() {
            awaker.notify();
        }
    }

    pub fn find_alias(&self, alias: NodeAliasId, handler: Box<dyn FnOnce(Result<NodeAliasResult, NodeAliasError>) + Send>) {
        log::info!("[NodeAliasSdk] Find alias: {}", alias);
        self.sdk_control_queue.lock().push_back(SdkControl::Query(alias, handler));
        if let Some(awaker) = &*self.awaker.lock() {
            awaker.notify();
        }
    }

    pub(crate) fn pop_control(&self) -> Option<SdkControl> {
        self.sdk_control_queue.lock().pop_front()
    }
}