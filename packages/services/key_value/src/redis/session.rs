use std::hash::{Hash, Hasher};

use async_std::net::TcpStream;
use async_std::prelude::*;
use utils::error_handle::ErrorUtils;

use super::cmd::RedisCmd;
use crate::KeyValueSdk;

fn key_hash(key: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

pub struct RedisSession {
    tcp_stream: TcpStream,
    sdk: KeyValueSdk,
}

impl RedisSession {
    pub fn new(sdk: KeyValueSdk, tcp_stream: TcpStream) -> Self {
        Self { tcp_stream, sdk }
    }

    async fn send_reply(&mut self, reply: resp::Value) -> Result<(), Box<dyn std::error::Error>> {
        self.tcp_stream.write(resp::encode(&reply).as_slice()).await?;
        Ok(())
    }

    async fn send_reply2(tcp_stream: &mut TcpStream, reply: resp::Value) -> Result<(), Box<dyn std::error::Error>> {
        tcp_stream.write(resp::encode(&reply).as_slice()).await?;
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = vec![0; 9000];
        let mut subscribe_task = None;
        while let Ok(len) = self.tcp_stream.read(&mut buf).await {
            if len == 0 {
                break;
            }
            let cmd = RedisCmd::try_from(&buf[..len]);
            match cmd {
                Ok(RedisCmd::Command(_cmd)) => {
                    self.send_reply(resp::Value::Error("Not implemented".to_string())).await?;
                }
                Ok(RedisCmd::Get(key)) => {
                    let key = key_hash(&key);
                    match self.sdk.get(key, 1000).await {
                        Ok(value) => match value {
                            Some(value) => {
                                if let Ok(value_str) = String::from_utf8(value.0) {
                                    self.send_reply(resp::Value::String(value_str)).await?;
                                } else {
                                    self.send_reply(resp::Value::Null).await?;
                                }
                            }
                            None => {
                                self.send_reply(resp::Value::Null).await?;
                            }
                        },
                        Err(e) => {
                            self.send_reply(resp::Value::Error(format!("{:?}", e))).await?;
                        }
                    }
                }
                Ok(RedisCmd::Del(key)) => {
                    let key = key_hash(&key);
                    self.sdk.del(key);
                    self.send_reply(resp::Value::String("OK".to_string())).await?;
                }
                Ok(RedisCmd::Set(key, value)) => {
                    let key = key_hash(&key);
                    self.sdk.set(key, value.as_bytes().to_vec(), None);
                    self.send_reply(resp::Value::String("OK".to_string())).await?;
                }
                Ok(RedisCmd::Sub(key)) => {
                    let mut stream = self.tcp_stream.clone();
                    let mut rx = self.sdk.subscribe(key_hash(&key), None);
                    subscribe_task = Some(async_std::task::spawn(async move {
                        while let Some((_, value, version)) = rx.recv().await {
                            log::debug!("recv: {:?}", value);
                            if let Some(value) = value {
                                Self::send_reply2(
                                    &mut stream,
                                    resp::Value::Array(vec![
                                        resp::Value::String("set".to_string()),
                                        resp::Value::String(key.clone()),
                                        resp::Value::String(String::from_utf8(value).unwrap()),
                                        resp::Value::Integer(version as i64),
                                    ]),
                                )
                                .await
                                .print_error("Should send event");
                            } else {
                                Self::send_reply2(
                                    &mut stream,
                                    resp::Value::Array(vec![resp::Value::String("del".to_string()), resp::Value::String(key.clone()), resp::Value::Integer(version as i64)]),
                                )
                                .await
                                .print_error("Should send event");
                            }
                        }
                    }));
                }
                Ok(RedisCmd::Unsub(key)) => {
                    self.send_reply(resp::Value::Error("NOT_SUPPORTED".to_string())).await?;
                }
                Err(_e) => {
                    self.send_reply(resp::Value::Error("NOT_SUPPORTED".to_string())).await?;
                }
            }
        }

        if let Some(subscribe_task) = subscribe_task.take() {
            subscribe_task.cancel().await;
        }

        Ok(())
    }
}
