use std::{net::SocketAddr, sync::atomic::AtomicBool};

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_network::{msg::TransportMsg, transport::ConnectionSender};
use atm0s_sdn_utils::error_handle::ErrorUtils;
use std::net::UdpSocket;
use std::sync::Arc;

use crate::msg::{build_control_msg, UdpTransportMsg};

pub struct UdpServerConnectionSender {
    remote_node_id: NodeId,
    remote_node_addr: NodeAddr,
    conn_id: ConnId,
    socket: Arc<UdpSocket>,
    socket_dest: SocketAddr,
    close_state: Arc<AtomicBool>,
    close_notify: Arc<async_notify::Notify>,
}

impl UdpServerConnectionSender {
    pub fn new(
        remote_node_id: NodeId,
        remote_node_addr: NodeAddr,
        conn_id: ConnId,
        socket: Arc<UdpSocket>,
        socket_dest: SocketAddr,
        close_state: Arc<AtomicBool>,
        close_notify: Arc<async_notify::Notify>,
    ) -> Self {
        log::info!("[UdpServerConnectionSender {}/{}] new", remote_node_id, conn_id);
        Self {
            remote_node_id,
            remote_node_addr,
            conn_id,
            socket,
            socket_dest,
            close_state,
            close_notify,
        }
    }
}

impl ConnectionSender for UdpServerConnectionSender {
    fn remote_node_id(&self) -> NodeId {
        self.remote_node_id
    }

    fn conn_id(&self) -> ConnId {
        self.conn_id
    }

    fn remote_addr(&self) -> NodeAddr {
        self.remote_node_addr.clone()
    }

    fn send(&self, msg: TransportMsg) {
        let buf = msg.take();
        self.socket.send_to(&buf, self.socket_dest).print_error("Send error");
    }

    fn close(&self) {
        //only process close procedue
        if self
            .close_state
            .compare_exchange(false, true, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        log::info!("[UdpServerConnectionSender {}/{}] close", self.remote_node_id, self.conn_id);
        self.socket.send_to(&build_control_msg(&UdpTransportMsg::Close), self.socket_dest).print_error("Send close error");
        self.close_notify.notify();
    }
}

impl Drop for UdpServerConnectionSender {
    fn drop(&mut self) {
        log::info!("[UdpServerConnectionSender {}/{}] drop", self.remote_node_id, self.conn_id);
        self.close();
    }
}

pub struct UdpClientConnectionSender {
    remote_node_id: NodeId,
    remote_node_addr: NodeAddr,
    conn_id: ConnId,
    socket: Arc<UdpSocket>,
    close_state: Arc<AtomicBool>,
    close_notify: Arc<async_notify::Notify>,
}

impl UdpClientConnectionSender {
    pub fn new(remote_node_id: NodeId, remote_node_addr: NodeAddr, conn_id: ConnId, socket: Arc<UdpSocket>, close_state: Arc<AtomicBool>, close_notify: Arc<async_notify::Notify>) -> Self {
        log::info!("[UdpClientConnectionSender {}/{}] new", remote_node_id, conn_id);
        Self {
            remote_node_id,
            remote_node_addr,
            conn_id,
            socket,
            close_state,
            close_notify,
        }
    }
}

impl ConnectionSender for UdpClientConnectionSender {
    fn remote_node_id(&self) -> NodeId {
        self.remote_node_id
    }

    fn conn_id(&self) -> ConnId {
        self.conn_id
    }

    fn remote_addr(&self) -> NodeAddr {
        self.remote_node_addr.clone()
    }

    fn send(&self, msg: TransportMsg) {
        let buf = msg.take();
        self.socket.send(&buf).print_error("Send error");
    }

    fn close(&self) {
        //only process close procedue
        if self
            .close_state
            .compare_exchange(false, true, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        log::info!("[UdpClientConnectionSender {}/{}] close", self.remote_node_id, self.conn_id);
        self.socket.send(&build_control_msg(&UdpTransportMsg::Close)).print_error("Send close error");
        self.close_notify.notify();
    }
}

impl Drop for UdpClientConnectionSender {
    fn drop(&mut self) {
        log::info!("[UdpClientConnectionSender {}/{}] drop", self.remote_node_id, self.conn_id);
        self.close();
    }
}
