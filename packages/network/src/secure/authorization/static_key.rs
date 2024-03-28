//! Simplest authorization method by using hash with salt is static key
//!

use atm0s_sdn_identity::NodeId;
use sha2::Digest;

use crate::base::Authorization;

pub struct StaticKeyAuthorization {
    key: String,
}

impl StaticKeyAuthorization {
    pub fn new(key: &str) -> Self {
        Self { key: key.to_string() }
    }
}

impl Authorization for StaticKeyAuthorization {
    /// Generate signature for message as hash with salt
    fn sign(&self, msg: &[u8]) -> Vec<u8> {
        let mut hasher = sha2::Sha256::default();
        hasher.update(msg);
        hasher.update(self.key.as_bytes());
        hasher.finalize().to_vec()
    }

    /// Validate message signature by comparing hash with salt
    fn validate(&self, _node_id: NodeId, msg: &[u8], sign: &[u8]) -> Option<()> {
        let mut hasher = sha2::Sha256::default();
        hasher.update(msg);
        hasher.update(self.key.as_bytes());
        let hash = hasher.finalize();
        if hash.as_slice() == sign {
            Some(())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_key_authorization() {
        let auth = StaticKeyAuthorization::new("demo-key");

        let msg = b"hello";
        let sign = auth.sign(msg);
        assert_eq!(auth.validate(1, msg, &sign), Some(()));
    }

    #[test]
    fn test_static_key_authorization_invalid() {
        let auth = StaticKeyAuthorization::new("demo-key");

        let msg = b"hello";
        let sign = auth.sign(msg);
        assert_eq!(auth.validate(1, msg, &sign[1..]), None);
    }
}
