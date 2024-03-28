use std::fmt::Debug;

use aes_gcm::{
    aead::{AeadMutInPlace, Buffer},
    AeadCore, Aes256Gcm, Key, KeyInit, Nonce,
};
use rand::rngs::OsRng;
use x25519_dalek::{EphemeralSecret, PublicKey};

use crate::base::{DecryptionError, Decryptor, EncryptionError, Encryptor, HandshakeBuilder, HandshakeError, HandshakeRequester, HandshakeResponder};

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
        Self { aes: Aes256Gcm::new(&key) }
    }
}

impl Encryptor for EncryptorXDA {
    fn encrypt(&mut self, now_ms: u64, data: &[u8], out: &mut [u8]) -> Result<usize, EncryptionError> {
        let mut nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        nonce[4..].copy_from_slice(&now_ms.to_be_bytes());
        out[0..12].copy_from_slice(&nonce);
        out[12..(12 + data.len())].copy_from_slice(data);
        let mut buf = SimpleMutBuf::new(&mut out[12..], data.len());
        self.aes.encrypt_in_place(&nonce, &[], &mut buf).map_err(|_| EncryptionError::EncryptFailed)?;
        Ok(12 + buf.len())
    }

    fn encrypt_vec(&mut self, now_ms: u64, data: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let mut out = vec![0u8; 12 + 16 + data.len()];
        let len = self.encrypt(now_ms, data, &mut out)?;
        out.truncate(len);
        Ok(out)
    }

    fn clone_box(&self) -> Box<dyn Encryptor> {
        Box::new(Self { aes: self.aes.clone() })
    }
}

struct DecryptorXDA {
    aes: Aes256Gcm,
}

impl DecryptorXDA {
    pub fn new(shared_key: &[u8; 32]) -> Self {
        let key = Key::<Aes256Gcm>::from_slice(shared_key);
        Self { aes: Aes256Gcm::new(&key) }
    }
}

impl Debug for DecryptorXDA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EncryptorXDA")
    }
}

impl Decryptor for DecryptorXDA {
    fn decrypt(&mut self, now_ms: u64, data: &[u8], out: &mut [u8]) -> Result<usize, DecryptionError> {
        if data.len() < 12 {
            return Err(DecryptionError::TooSmall);
        }
        let nonce = Nonce::from_slice(&data[..12]);
        let sent_ts = u64::from_be_bytes(data[4..12].try_into().expect("should be 8 bytes"));
        if sent_ts + MSG_TIMEOUT_MS < now_ms {
            return Err(DecryptionError::TooOld);
        }
        out[..(data.len() - 12)].copy_from_slice(&data[12..data.len()]);
        let mut encrypted_buf = SimpleMutBuf::new(out, data.len() - 12);
        self.aes.decrypt_in_place(nonce, &[], &mut encrypted_buf).map_err(|_| DecryptionError::TooSmall)?;
        Ok(encrypted_buf.len())
    }

    fn decrypt_vec(&mut self, now_ms: u64, data: &[u8]) -> Result<Vec<u8>, DecryptionError> {
        let mut out = vec![0u8; data.len()];
        let len = self.decrypt(now_ms, data, &mut out)?;
        out.truncate(len);
        Ok(out)
    }

    fn clone_box(&self) -> Box<dyn Decryptor> {
        Box::new(Self { aes: self.aes.clone() })
    }
}

struct SimpleMutBuf<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl<'a> SimpleMutBuf<'a> {
    fn new(value: &'a mut [u8], len: usize) -> Self {
        Self { buf: value, len }
    }
}

impl<'a> Buffer for SimpleMutBuf<'a> {
    fn extend_from_slice(&mut self, other: &[u8]) -> aes_gcm::aead::Result<()> {
        if self.buf.len() < self.len + other.len() {
            println!("Buffer is too small {}, {} extend with {}", self.buf.len(), self.len, other.len());
            return Err(aes_gcm::aead::Error);
        }
        self.buf[self.len..(self.len + other.len())].copy_from_slice(other);
        self.len += other.len();
        Ok(())
    }

    fn truncate(&mut self, len: usize) {
        println!("Truncate to {} from {}", len, self.len);
        self.len = len;
    }

    fn len(&self) -> usize {
        self.len
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<'a> AsRef<[u8]> for SimpleMutBuf<'a> {
    fn as_ref(&self) -> &[u8] {
        &self.buf[0..self.len]
    }
}

impl<'a> AsMut<[u8]> for SimpleMutBuf<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.buf[0..self.len]
    }
}

#[cfg(test)]
mod tests {
    use crate::base::{HandshakeRequester, HandshakeResponder};

    use super::{HandshakeRequesterXDA, HandshakeResponderXDA};

    #[test]
    fn simple_encryption() {
        let mut client = HandshakeRequesterXDA::default();
        let mut server = HandshakeResponderXDA::default();

        let (mut s_encrypt, mut s_decrypt, res) = server.process_public_request(client.create_public_request().expect("").as_slice()).expect("Should ok");
        let (mut c_encrypt, mut c_decrypt) = client.process_public_response(res.as_slice()).expect("Should ok");

        let msg = [1, 2, 3, 4];

        let encrypted = s_encrypt.encrypt_vec(123, &msg).expect("Should ok");
        let decrypted = c_decrypt.decrypt_vec(124, &encrypted).expect("Should ok");
        assert_eq!(decrypted, msg);

        let encrypted = c_encrypt.encrypt_vec(123, &msg).expect("Should ok");
        let decrypted = s_decrypt.decrypt_vec(124, &encrypted).expect("Should ok");
        assert_eq!(decrypted, msg);
    }

    #[test]
    fn unordered_encryption() {
        let mut client = HandshakeRequesterXDA::default();
        let mut server = HandshakeResponderXDA::default();

        let (mut s_encrypt, _s_decrypt, res) = server.process_public_request(client.create_public_request().expect("").as_slice()).expect("Should ok");
        let (_c_encrypt, mut c_decrypt) = client.process_public_response(res.as_slice()).expect("Should ok");

        let encrypted1 = s_encrypt.encrypt_vec(123, &[0, 0, 0, 1]).expect("Should ok");
        let encrypted2 = s_encrypt.encrypt_vec(124, &[0, 0, 0, 2]).expect("Should ok");
        let encrypted3 = s_encrypt.encrypt_vec(125, &[0, 0, 0, 3]).expect("Should ok");

        let decrypted1 = c_decrypt.decrypt_vec(123, &encrypted1).expect("Should ok");
        let decrypted3 = c_decrypt.decrypt_vec(125, &encrypted3).expect("Should ok");
        let decrypted2 = c_decrypt.decrypt_vec(124, &encrypted2).expect("Should ok");

        assert_eq!(decrypted1, [0, 0, 0, 1]);
        assert_eq!(decrypted2, [0, 0, 0, 2]);
        assert_eq!(decrypted3, [0, 0, 0, 3]);
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
            let encrypted = s_enc_threads[i as usize % ENC_THREADS].encrypt_vec(i as u64, &msg).expect("Should ok");
            let decrypted = c_dec_threads[i as usize % DEC_THREADS].decrypt_vec(i as u64, &encrypted).expect("Should ok");
            assert_eq!(decrypted, msg);
        }
    }
}
