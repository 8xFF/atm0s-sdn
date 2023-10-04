use std::net::SocketAddr;

use async_std::net::TcpListener;

use crate::{redis::session::RedisSession, KeyValueSdk};

pub(crate) mod cmd;
mod session;

pub struct RedisServer {
    addr: SocketAddr,
    sdk: KeyValueSdk,
}

impl RedisServer {
    pub fn new(addr: SocketAddr, sdk: KeyValueSdk) -> Self {
        Self { addr, sdk }
    }

    pub async fn run(&mut self) {
        let socket = TcpListener::bind(self.addr).await.expect("Should bind");
        log::info!("[RedisServer] listen on {}", self.addr);
        while let Ok((stream, addr)) = socket.accept().await {
            log::info!("[RedisServer] accept connection from {}", addr);
            let mut session = RedisSession::new(self.sdk.clone(), stream);
            async_std::task::spawn(async move {
                if let Err(e) = session.run().await {
                    log::error!("[RedisServer] session error {:?}", e);
                }
            });
            log::info!("[RedisServer] closed connection from {}", addr);
        }
    }
}
