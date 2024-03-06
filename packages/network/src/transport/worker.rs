use std::collections::HashMap;

use super::{conn_addr::ConnAddr, conn_id::ConnId, manager::TransportManagerInput, msg::TransportMsg, TransportBuffer, TransportProtocol};

pub enum TransportWorkerInput<'a> {
    SendTo(ConnId, &'a [u8]),
    Received(TransportProtocol, SocketAddr, &'a [u8]),
    PinConn(ConnId, ConnAddr),
    UnpinConn(ConnId),
}

pub enum TransportWorkerOut<'a> {
    SendTo(TransportProtocol, SocketAddr, &'a [u8]),
    PassConnection(ConnId, TransportMsg<'a>),
    PassManager(TransportManagerInput<'a>),
}

pub struct TransportWorker {
    conns: HashMap<ConnId, ConnAddr>,
    conns_reverse: HashMap<ConnAddr, ConnId>,
} 

impl TransportWorker  {
    pub fn on_tick(&mut self, now_ms: u64) -> Option<TransportWorkerOut> {
        None
    }
    pub fn on_event(&mut self, now_ms: u64, event: TransportWorkerInput) -> Option<TransportWorkerOut> {
        match event {
            TransportWorkerInput::PinConn(conn, addr) => {
                self.conns.insert(conn, addr);
                self.conns_reverse.insert(addr, conn);
                None
            },
            TransportWorkerInput::UnpinConn(conn) => {
                if let Some(addr) = self.conns.remove(&conn) {
                    self.conns_reverse.remove(&addr);
                }
                None
            },
            TransportWorkerInput::Received(protocol, addr, buf) => {
                let addr = ConnAddr(protocol, addr);
                if let Some(conn) = self.conns_reverse.get(&addr) {
                    let msg = TransportMsg::from_ref(buf);
                    Some(TransportWorkerOut::PassConnection(*conn, msg))
                } else {
                    None
                }
            },
            TransportWorkerInput::SendTo(conn, buf) => {
                if let Some(addr) = self.conns.get(&conn) {
                    Some(TransportWorkerOut::SendTo(addr.0, addr.1, buf))
                } else {
                    None
                }
            }
        }
    }
    pub fn pop_output(&mut self) -> Option<TransportWorkerOut> {
        None
    }
}