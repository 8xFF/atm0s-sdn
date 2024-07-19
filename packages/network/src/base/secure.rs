use std::fmt::Debug;

use atm0s_sdn_identity::NodeId;

use super::Buffer;

#[derive(Debug, Clone)]
pub struct SecureContext {
    pub(crate) encryptor: Box<dyn Encryptor>,
    pub(crate) decryptor: Box<dyn Decryptor>,
}

#[mockall::automock]
pub trait Authorization: Send + Sync {
    fn sign(&self, msg: &[u8]) -> Vec<u8>;
    fn validate(&self, node_id: NodeId, msg: &[u8], sign: &[u8]) -> Option<()>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum HandshakeError {
    InvalidState,
    InvalidPublicKey,
}

#[mockall::automock]
pub trait HandshakeBuilder: Send + Sync {
    fn requester(&self) -> Box<dyn HandshakeRequester>;
    fn responder(&self) -> Box<dyn HandshakeResponder>;
}

#[mockall::automock]
pub trait HandshakeRequester {
    fn create_public_request(&self) -> Result<Vec<u8>, HandshakeError>;
    #[allow(clippy::type_complexity)]
    fn process_public_response(&mut self, response: &[u8]) -> Result<(Box<dyn Encryptor>, Box<dyn Decryptor>), HandshakeError>;
}

#[mockall::automock]
pub trait HandshakeResponder {
    #[allow(clippy::type_complexity)]
    fn process_public_request(&mut self, request: &[u8]) -> Result<(Box<dyn Encryptor>, Box<dyn Decryptor>, Vec<u8>), HandshakeError>;
}

#[derive(Debug)]
pub enum EncryptionError {
    EncryptFailed,
}

#[mockall::automock]
pub trait Encryptor: Debug + Send + Sync {
    fn encrypt(&mut self, now_ms: u64, data: &mut Buffer) -> Result<(), EncryptionError>;
    fn clone_box(&self) -> Box<dyn Encryptor>;
}

impl Clone for Box<dyn Encryptor> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

#[derive(Debug)]
pub enum DecryptionError {
    TooSmall,
    TooOld,
    DecryptError,
}

#[mockall::automock]
pub trait Decryptor: Debug + Send + Sync {
    fn decrypt(&mut self, now_ms: u64, data: &mut Buffer) -> Result<(), DecryptionError>;
    fn clone_box(&self) -> Box<dyn Decryptor>;
}

impl Clone for Box<dyn Decryptor> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
