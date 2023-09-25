use std::{
    os::fd::{AsRawFd, FromRawFd},
    sync::Arc,
};

use async_std::{
    channel::{Receiver, Sender},
    fs::File,
    io::{ReadExt, WriteExt},
};
use bluesea_identity::{ConnId, NodeId};
use bluesea_router::RouteRule;
use futures::{select, FutureExt};
use network::{
    behaviour::{ConnectionHandler, NetworkBehavior},
    msg::TransportMsg,
    transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, RpcAnswer},
    BehaviorAgent,
};
use utils::{error_handle::ErrorUtils, option_handle::OptionUtils};

use crate::{TunTapBehaviorEvent, TunTapHandler, TunTapHandlerEvent, TunTapReq, TunTapRes, TUNTAP_SERVICE_ID};

pub struct TunTapBehavior {
    join: Option<async_std::task::JoinHandle<()>>,
    local_tx: Sender<TransportMsg>,
    local_rx: Option<Receiver<TransportMsg>>,
}

impl Default for TunTapBehavior {
    fn default() -> Self {
        let (local_tx, local_rx) = async_std::channel::bounded(100);
        Self {
            join: None,
            local_tx,
            local_rx: Some(local_rx),
        }
    }
}

impl<BE, HE, Req, Res> NetworkBehavior<BE, HE, Req, Res> for TunTapBehavior
where
    BE: From<TunTapBehaviorEvent> + TryInto<TunTapBehaviorEvent> + Send + Sync + 'static,
    HE: From<TunTapHandlerEvent> + TryInto<TunTapHandlerEvent> + Send + Sync + 'static,
    Req: From<TunTapReq> + TryInto<TunTapReq> + Send + Sync + 'static,
    Res: From<TunTapRes> + TryInto<TunTapRes> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        TUNTAP_SERVICE_ID
    }

    fn on_started(&mut self, agent: &BehaviorAgent<HE>) {
        if let Some(rx) = self.local_rx.take() {
            let mut config = tun::Configuration::default();
            config
                .address((10, 0, 0, (agent.local_node_id() % 256) as u8)) //TODO using ipv6 instead
                .netmask((255, 255, 255, 0))
                .mtu(1180)
                .up();

            #[cfg(target_os = "linux")]
            config.platform(|config| {
                config.packet_information(true);
            });

            let dev = tun::create(&config).unwrap();
            let mut async_file = unsafe { File::from_raw_fd(dev.as_raw_fd()) };
            let mut buf = [0; 4096];

            let agent = agent.clone();
            let join = async_std::task::spawn(async move {
                let agent = agent.clone();
                loop {
                    select! {
                        e = async_file.read(&mut buf).fuse() => match e {
                            Ok(amount) => {
                                let to_ip = &buf[20..24];
                                let dest = NodeId::from(to_ip[3] as u32);
                                agent.send_to_net(TransportMsg::build_unreliable(TUNTAP_SERVICE_ID, RouteRule::ToNode(dest), 0, &buf[0..amount]));
                            },
                            Err(e) => {
                                log::error!("read tun error {}", e);
                                break;
                            }
                        },
                        msg = rx.recv().fuse() => {
                            if let Ok(msg) = msg {
                                async_file.write(msg.payload()).await.print_error("write tun error");
                            } else {
                                break;
                            }
                        }
                    };
                }
            });
            self.join = Some(join)
        }
    }

    fn on_tick(&mut self, _agent: &BehaviorAgent<HE>, _ts_ms: u64, _interal_ms: u64) {}

    fn check_incoming_connection(&mut self, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(&mut self, _agent: &BehaviorAgent<HE>, _conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(TunTapHandler { local_tx: self.local_tx.clone() }))
    }

    fn on_outgoing_connection_connected(&mut self, _agent: &BehaviorAgent<HE>, _connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(TunTapHandler { local_tx: self.local_tx.clone() }))
    }

    fn on_incoming_connection_disconnected(&mut self, _agent: &BehaviorAgent<HE>, _connection: Arc<dyn ConnectionSender>) {}

    fn on_outgoing_connection_disconnected(&mut self, _agent: &BehaviorAgent<HE>, _connection: Arc<dyn ConnectionSender>) {}

    fn on_outgoing_connection_error(&mut self, _agent: &BehaviorAgent<HE>, _node_id: NodeId, _conn_id: ConnId, _err: &OutgoingConnectionError) {}

    fn on_handler_event(&mut self, _agent: &BehaviorAgent<HE>, _node_id: NodeId, _connection_id: ConnId, _event: BE) {}

    fn on_rpc(&mut self, _agent: &BehaviorAgent<HE>, _req: Req, _res: Box<dyn RpcAnswer<Res>>) -> bool {
        false
    }

    fn on_stopped(&mut self, _agent: &BehaviorAgent<HE>) {}
}

impl Drop for TunTapBehavior {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            async_std::task::spawn(async move {
                join.cancel().await.print_none("Should cancel task");
            });
        }
    }
}
