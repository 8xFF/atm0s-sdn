use atm0s_sdn_identity::NodeId;

#[derive(Debug, Clone)]
pub struct SecureContext {}

pub trait Authorization {
    fn sign(&self, msg: &[u8]) -> Vec<u8>;
    fn validate(&self, node_id: NodeId, msg: &[u8], sign: &[u8]) -> Option<()>;
}
