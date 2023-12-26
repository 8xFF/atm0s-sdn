use std::fmt::Debug;

use atm0s_sdn_identity::NodeId;
use serde::Serialize;

mod static_key;

pub trait DataSecure: Send + Sync {
    fn sign_msg(&self, remote_node_id: NodeId, data: &[u8]) -> Vec<u8>;
    fn verify_msg(&self, remote_node_id: NodeId, data: &[u8], signature: &[u8]) -> bool;
}

pub struct ObjectSecure;

impl ObjectSecure {
    pub fn sign_obj<T: Debug + Serialize>(secure: &dyn DataSecure, remote_node_id: NodeId, obj: &T) -> Vec<u8> {
        log::info!("sign obj {:?} for remote {}", obj, remote_node_id);
        secure.sign_msg(remote_node_id, &bincode::serialize(obj).expect("Shoukd serialize obj"))
    }
    pub fn verify_obj<T: Debug + Serialize>(secure: &dyn DataSecure, remote_node_id: NodeId, obj: &T, signature: &[u8]) -> bool {
        log::info!("verify obj {:?} for remote {}", obj, remote_node_id);
        secure.verify_msg(remote_node_id, &bincode::serialize(obj).expect("Shoukd serialize obj"), signature)
    }
}

pub use static_key::StaticKeySecure;
