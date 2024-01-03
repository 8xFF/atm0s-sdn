use std::{collections::HashMap, sync::Arc};

use async_std::channel::Receiver;
use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::msg::{MsgHeader, TransportMsg};
use atm0s_sdn_router::RouteRule;
use parking_lot::RwLock;

use crate::{
    msg::{SocketId, VirtualSocketControlMsg},
    VIRTUAL_SOCKET_SERVICE_ID,
};

use super::State;

pub const CONTROL_CLIENT_META: u8 = 0;
pub const CONTROL_SERVER_META: u8 = 1;
pub const DATA_CLIENT_META: u8 = 2;
pub const DATA_SERVER_META: u8 = 3;

pub struct VirtualSocketBuilder {
    is_client: bool,
    remote: SocketId,
    rx: Receiver<Vec<u8>>,
    meta: HashMap<String, String>,
}

impl VirtualSocketBuilder {
    pub(crate) fn new(is_client: bool, remote: SocketId, meta: HashMap<String, String>, rx: Receiver<Vec<u8>>) -> Self {
        Self { is_client, remote, meta, rx }
    }

    pub fn build(self, state: Arc<RwLock<State>>) -> VirtualSocket {
        VirtualSocket::new(self.is_client, self.remote, self.meta, self.rx, state)
    }
}

#[derive(Debug)]
pub enum VirtualSocketEvent {
    ServerControl(VirtualSocketControlMsg),
    ClientControl(VirtualSocketControlMsg),
    ServerData(Vec<u8>),
    ClientData(Vec<u8>),
}

impl VirtualSocketEvent {
    pub fn into_transport_msg(self, local_node: NodeId, remote_node: NodeId, client_id: u32) -> TransportMsg {
        match self {
            VirtualSocketEvent::ServerData(data) => {
                let header = MsgHeader::build(VIRTUAL_SOCKET_SERVICE_ID, VIRTUAL_SOCKET_SERVICE_ID, RouteRule::ToNode(remote_node))
                    .set_from_node(Some(local_node))
                    .set_stream_id(client_id)
                    .set_meta(DATA_SERVER_META);
                TransportMsg::build_raw(header, &data)
            }
            VirtualSocketEvent::ClientData(data) => {
                let header = MsgHeader::build(VIRTUAL_SOCKET_SERVICE_ID, VIRTUAL_SOCKET_SERVICE_ID, RouteRule::ToNode(remote_node))
                    .set_from_node(Some(local_node))
                    .set_stream_id(client_id)
                    .set_meta(DATA_CLIENT_META);
                TransportMsg::build_raw(header, &data)
            }
            VirtualSocketEvent::ServerControl(control) => {
                let header = MsgHeader::build(VIRTUAL_SOCKET_SERVICE_ID, VIRTUAL_SOCKET_SERVICE_ID, RouteRule::ToNode(remote_node))
                    .set_from_node(Some(local_node))
                    .set_stream_id(client_id)
                    .set_meta(CONTROL_SERVER_META);
                TransportMsg::from_payload_bincode(header, &control)
            }
            VirtualSocketEvent::ClientControl(control) => {
                let header = MsgHeader::build(VIRTUAL_SOCKET_SERVICE_ID, VIRTUAL_SOCKET_SERVICE_ID, RouteRule::ToNode(remote_node))
                    .set_from_node(Some(local_node))
                    .set_stream_id(client_id)
                    .set_meta(CONTROL_CLIENT_META);
                TransportMsg::from_payload_bincode(header, &control)
            }
        }
    }
}

pub struct VirtualSocket {
    is_client: bool,
    remote: SocketId,
    rx: Receiver<Vec<u8>>,
    meta: HashMap<String, String>,
    state: Arc<RwLock<State>>,
}

impl VirtualSocket {
    pub(crate) fn new(is_client: bool, remote: SocketId, meta: HashMap<String, String>, rx: Receiver<Vec<u8>>, state: Arc<RwLock<State>>) -> Self {
        Self { is_client, remote, meta, rx, state }
    }

    pub fn remote(&self) -> &SocketId {
        &self.remote
    }

    pub fn meta(&self) -> &HashMap<String, String> {
        &self.meta
    }

    pub async fn read(&mut self) -> Option<Vec<u8>> {
        self.rx.recv().await.ok()
    }

    pub async fn write(&mut self, buf: Vec<u8>) -> Option<()> {
        if self.is_client {
            self.state.write().send_out(self.remote.clone(), VirtualSocketEvent::ClientData(buf));
        } else {
            self.state.write().send_out(self.remote.clone(), VirtualSocketEvent::ServerData(buf));
        }
        Some(())
    }
}

impl Drop for VirtualSocket {
    fn drop(&mut self) {
        self.state.write().close_socket(self.is_client, &self.remote);
    }
}