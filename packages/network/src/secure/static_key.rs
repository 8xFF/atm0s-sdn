use atm0s_sdn_identity::NodeId;
use sha1::{Digest, Sha1};

use super::DataSecure;

pub struct StaticKeySecure {
    key: String,
}

impl StaticKeySecure {
    pub fn new(key: &str) -> Self {
        Self { key: key.to_string() }
    }
}

impl DataSecure for StaticKeySecure {
    fn sign_msg(&self, remote_node_id: NodeId, data: &[u8]) -> Vec<u8> {
        let mut hasher = Sha1::new();
        hasher.update(data);
        hasher.update(remote_node_id.to_be_bytes());
        hasher.update(self.key.clone().into_bytes());
        hasher.finalize().to_vec()
    }

    fn verify_msg(&self, remote_node_id: NodeId, data: &[u8], signature: &[u8]) -> bool {
        let mut hasher = Sha1::new();
        hasher.update(data);
        hasher.update(remote_node_id.to_be_bytes());
        hasher.update(self.key.clone().into_bytes());
        hasher.finalize().to_vec() == signature
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_sign_verify() {
        let secure = StaticKeySecure::new("secure-token");
        let data = b"Hello World";
        let signature = secure.sign_msg(1, data);
        assert!(secure.verify_msg(1, data, &signature));
        assert!(!secure.verify_msg(2, data, &signature));
    }

    #[test]
    fn test_two_msg_different_signatures() {
        let secure = StaticKeySecure::new("secure-token");
        let data1 = b"Hello World";
        let data2 = b"Hello World 2";
        let signature1 = secure.sign_msg(1, data1);
        let signature2 = secure.sign_msg(1, data2);
        assert_ne!(signature1, signature2);
    }
}
