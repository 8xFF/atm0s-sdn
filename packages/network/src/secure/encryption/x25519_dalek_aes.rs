use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use aes_gcm::{
    aead::{AeadMutInPlace, Buffer},
    AeadCore, Aes256Gcm, Key, KeyInit, Nonce,
};
use rand::rngs::OsRng;
use x25519_dalek::{EphemeralSecret, PublicKey};

use crate::base::{DecryptionError, Decryptor, EncryptionError, Encryptor, GenericBufferMut, HandshakeBuilder, HandshakeError, HandshakeRequester, HandshakeResponder};

const MSG_TIMEOUT_MS: u64 = 5000; // after 5 seconds message is considered expired

pub struct HandshakeBuilderXDA;

impl HandshakeBuilder for HandshakeBuilderXDA {
    fn requester(&self) -> Box<dyn HandshakeRequester> {
        Box::new(HandshakeRequesterXDA::default())
    }

    fn responder(&self) -> Box<dyn HandshakeResponder> {
        Box::new(HandshakeResponderXDA::default())
    }
}

pub struct HandshakeRequesterXDA {
    key: Option<EphemeralSecret>,
}

impl Default for HandshakeRequesterXDA {
    fn default() -> Self {
        Self { key: Some(EphemeralSecret::random()) }
    }
}

impl HandshakeRequester for HandshakeRequesterXDA {
    fn create_public_request(&self) -> Result<Vec<u8>, HandshakeError> {
        let key = self.key.as_ref().ok_or(HandshakeError::InvalidState)?;
        Ok(PublicKey::from(key).as_bytes().to_vec())
    }

    fn process_public_response(&mut self, response: &[u8]) -> Result<(Box<dyn Encryptor>, Box<dyn Decryptor>), HandshakeError> {
        let buf: [u8; 32] = response.try_into().map_err(|_| HandshakeError::InvalidPublicKey)?;
        let public = PublicKey::from(buf);
        let shared_key = self.key.take().ok_or(HandshakeError::InvalidState)?.diffie_hellman(&public);
        Ok((Box::new(EncryptorXDA::new(shared_key.as_bytes())), Box::new(DecryptorXDA::new(shared_key.as_bytes()))))
    }
}

pub struct HandshakeResponderXDA {
    key: Option<EphemeralSecret>,
}

impl Default for HandshakeResponderXDA {
    fn default() -> Self {
        Self { key: Some(EphemeralSecret::random()) }
    }
}

impl HandshakeResponder for HandshakeResponderXDA {
    fn process_public_request(&mut self, request: &[u8]) -> Result<(Box<dyn Encryptor>, Box<dyn Decryptor>, Vec<u8>), HandshakeError> {
        let buf: [u8; 32] = request.try_into().map_err(|_| HandshakeError::InvalidPublicKey)?;
        let key = self.key.take().ok_or(HandshakeError::InvalidState)?;
        let public = PublicKey::from(buf);
        let response = PublicKey::from(&key).as_bytes().to_vec();
        let shared_key = key.diffie_hellman(&public);
        Ok((Box::new(EncryptorXDA::new(shared_key.as_bytes())), Box::new(DecryptorXDA::new(shared_key.as_bytes())), response))
    }
}

struct EncryptorXDA {
    key: Vec<u8>,
    aes: Aes256Gcm,
}

impl Debug for EncryptorXDA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EncryptorXDA")
    }
}

impl EncryptorXDA {
    pub fn new(shared_key: &[u8; 32]) -> Self {
        let key = Key::<Aes256Gcm>::from_slice(shared_key);
        Self {
            key: shared_key.to_vec(),
            aes: Aes256Gcm::new(&key),
        }
    }
}

impl Encryptor for EncryptorXDA {
    fn encrypt<'a>(&mut self, now_ms: u64, buf: &mut GenericBufferMut<'a>) -> Result<(), EncryptionError> {
        let mut nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        nonce[4..].copy_from_slice(&now_ms.to_be_bytes());
        self.aes.encrypt_in_place(&nonce, &[], buf).map_err(|_| EncryptionError::EncryptFailed)?;
        buf.extend_from_slice(&nonce).map_err(|_| EncryptionError::EncryptFailed)?;
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn Encryptor> {
        Box::new(Self {
            aes: self.aes.clone(),
            key: self.key.clone(),
        })
    }
}

struct DecryptorXDA {
    key: Vec<u8>,
    aes: Aes256Gcm,
}

impl DecryptorXDA {
    pub fn new(shared_key: &[u8; 32]) -> Self {
        let key = Key::<Aes256Gcm>::from_slice(shared_key);
        Self {
            key: shared_key.to_vec(),
            aes: Aes256Gcm::new(&key),
        }
    }
}

impl Debug for DecryptorXDA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EncryptorXDA")
    }
}

impl Decryptor for DecryptorXDA {
    fn decrypt<'a>(&mut self, now_ms: u64, data: &mut GenericBufferMut<'a>) -> Result<(), DecryptionError> {
        let nonce = if let Some(nonce) = data.pop_back(12) {
            nonce.to_vec()
        } else {
            return Err(DecryptionError::TooSmall);
        };
        let sent_ts = u64::from_be_bytes(nonce[4..12].try_into().expect("should be 8 bytes"));
        if sent_ts + MSG_TIMEOUT_MS < now_ms {
            return Err(DecryptionError::TooOld);
        }
        let nonce = Nonce::from_slice(&nonce);
        self.aes.decrypt_in_place(nonce, &[], data).map_err(|_| DecryptionError::DecryptError)?;
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn Decryptor> {
        Box::new(Self {
            aes: self.aes.clone(),
            key: self.key.clone(),
        })
    }
}

impl<'a> Buffer for GenericBufferMut<'a> {
    fn extend_from_slice(&mut self, other: &[u8]) -> aes_gcm::aead::Result<()> {
        self.extend(other).ok_or(aes_gcm::aead::Error)
    }

    fn truncate(&mut self, len: usize) {
        GenericBufferMut::truncate(self, len).expect("Should truncate ok");
    }

    fn len(&self) -> usize {
        self.deref().len()
    }

    fn is_empty(&self) -> bool {
        self.deref().is_empty()
    }
}

impl<'a> AsRef<[u8]> for GenericBufferMut<'a> {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl<'a> AsMut<[u8]> for GenericBufferMut<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.deref_mut()
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Deref;

    use crate::base::{GenericBufferMut, HandshakeRequester, HandshakeResponder};

    use super::{HandshakeRequesterXDA, HandshakeResponderXDA};

    #[test]
    fn simple_encryption() {
        let mut client = HandshakeRequesterXDA::default();
        let mut server = HandshakeResponderXDA::default();

        let (mut s_encrypt, mut s_decrypt, res) = server.process_public_request(client.create_public_request().expect("").as_slice()).expect("Should ok");
        let (mut c_encrypt, mut c_decrypt) = client.process_public_response(res.as_slice()).expect("Should ok");

        let msg = [1, 2, 3, 4];

        let mut buf1 = GenericBufferMut::create_from_slice(&msg, 0, 1000);
        s_encrypt.encrypt(123, &mut buf1).expect("Should ok");
        assert_ne!(buf1.len(), msg.len());
        c_decrypt.decrypt(124, &mut buf1).expect("Should ok");
        assert_eq!(buf1.deref(), msg);

        let mut buf2 = GenericBufferMut::create_from_slice(&msg, 0, 1000);
        c_encrypt.encrypt(123, &mut buf2).expect("Should ok");
        assert_ne!(buf2.len(), msg.len());
        s_decrypt.decrypt(124, &mut buf2).expect("Should ok");
        assert_eq!(buf2.deref(), msg);
    }

    #[test]
    fn unordered_encryption() {
        let mut client = HandshakeRequesterXDA::default();
        let mut server = HandshakeResponderXDA::default();

        let (mut s_encrypt, _s_decrypt, res) = server.process_public_request(client.create_public_request().expect("").as_slice()).expect("Should ok");
        let (_c_encrypt, mut c_decrypt) = client.process_public_response(res.as_slice()).expect("Should ok");

        let mut buf1 = GenericBufferMut::create_from_slice(&[0, 0, 0, 1], 0, 1000);
        s_encrypt.encrypt(123, &mut buf1).expect("Should ok");
        let mut buf2 = GenericBufferMut::create_from_slice(&[0, 0, 0, 2], 0, 1000);
        s_encrypt.encrypt(123, &mut buf2).expect("Should ok");
        let mut buf3 = GenericBufferMut::create_from_slice(&[0, 0, 0, 3], 0, 1000);
        s_encrypt.encrypt(123, &mut buf3).expect("Should ok");

        c_decrypt.decrypt(123, &mut buf1).expect("Should ok");
        c_decrypt.decrypt(123, &mut buf2).expect("Should ok");
        c_decrypt.decrypt(123, &mut buf3).expect("Should ok");

        assert_eq!(buf1.deref(), &[0, 0, 0, 1]);
        assert_eq!(buf2.deref(), &[0, 0, 0, 2]);
        assert_eq!(buf3.deref(), &[0, 0, 0, 3]);
    }

    #[test]
    fn multi_thread_encyption_simulate() {
        let mut client = HandshakeRequesterXDA::default();
        let mut server = HandshakeResponderXDA::default();

        let (s_encrypt, _s_decrypt, res) = server.process_public_request(client.create_public_request().expect("").as_slice()).expect("Should ok");
        let (_c_encrypt, c_decrypt) = client.process_public_response(res.as_slice()).expect("Should ok");

        let mut s_enc_threads = Vec::new();
        let mut c_dec_threads = Vec::new();

        const ENC_THREADS: usize = 10;
        const DEC_THREADS: usize = 4;

        for _ in 0..ENC_THREADS {
            s_enc_threads.push(s_encrypt.clone_box());
        }

        for _ in 0..DEC_THREADS {
            c_dec_threads.push(c_decrypt.clone_box());
        }

        for i in 0..1024 {
            let value: u32 = i;
            let msg = value.to_be_bytes();
            let mut buf = GenericBufferMut::create_from_slice(&msg, 0, 1000);
            s_enc_threads[i as usize % ENC_THREADS].encrypt(i as u64, &mut buf).expect("Should ok");
            c_dec_threads[i as usize % DEC_THREADS].decrypt(i as u64, &mut buf).expect("Should ok");
            assert_eq!(buf.deref(), msg);
        }
    }
}
