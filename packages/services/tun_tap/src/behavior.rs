use std::{
    collections::VecDeque,
    io::Write,
    net::{IpAddr, Ipv4Addr},
    os::fd::{AsRawFd, FromRawFd},
    sync::Arc,
};

use async_std::{
    channel::{Receiver, Sender},
    fs::File,
    io::ReadExt,
    process::Command,
};
use atm0s_sdn_identity::{ConnId, NodeId, NodeIdType};
use atm0s_sdn_network::{
    behaviour::{BehaviorContext, ConnectionHandler, NetworkBehavior, NetworkBehaviorAction},
    msg::TransportMsg,
    transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, TransportOutgoingLocalUuid},
};
use atm0s_sdn_router::RouteRule;
use atm0s_sdn_utils::{error_handle::ErrorUtils, option_handle::OptionUtils};
use futures::{select, FutureExt};
use parking_lot::RwLock;

use crate::{TunTapBehaviorEvent, TunTapHandler, TunTapHandlerEvent, TUNTAP_SERVICE_ID};

pub struct TunTapBehavior<HE, SE> {
    join: Option<async_std::task::JoinHandle<()>>,
    local_tx: Sender<TransportMsg>,
    local_rx: Option<Receiver<TransportMsg>>,
    actions: Arc<RwLock<VecDeque<NetworkBehaviorAction<HE, SE>>>>,
}

impl<HE, SE> Default for TunTapBehavior<HE, SE> {
    fn default() -> Self {
        let (local_tx, local_rx) = async_std::channel::bounded(1000);
        Self {
            join: None,
            local_tx,
            local_rx: Some(local_rx),
            actions: Default::default(),
        }
    }
}

impl<BE, HE, SE> NetworkBehavior<BE, HE, SE> for TunTapBehavior<HE, SE>
where
    BE: From<TunTapBehaviorEvent> + TryInto<TunTapBehaviorEvent> + Send + Sync + 'static,
    HE: From<TunTapHandlerEvent> + TryInto<TunTapHandlerEvent> + Send + Sync + 'static,
    SE: Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        TUNTAP_SERVICE_ID
    }

    fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE, SE>> {
        self.actions.write().pop_front()
    }

    fn on_awake(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}
    fn on_sdk_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _from_service: u8, _event: SE) {}

    fn on_started(&mut self, ctx: &BehaviorContext, _now_ms: u64) {
        if let Some(rx) = self.local_rx.take() {
            let ctx = ctx.clone();
            let actions = self.actions.clone();
            let join = async_std::task::spawn(async move {
                let mut config = tun_sync::Configuration::default();
                let node_id = ctx.node_id.clone();
                let ip_addr: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 33, node_id.layer(1), node_id.layer(0)));

                config
                    .address(ip_addr.clone()) //TODO using ipv6 instead
                    .destination(ip_addr.clone())
                    .netmask((255, 255, 0, 0))
                    .mtu(1180)
                    .up();

                #[cfg(target_os = "linux")]
                config.platform(|config| {
                    config.packet_information(true);
                });

                let mut dev: tun_sync::platform::Device = tun_sync::create(&config).unwrap();
                log::info!("created tun device fd {}", dev.as_raw_fd());

                #[cfg(any(target_os = "macos", target_os = "ios"))]
                {
                    let output = Command::new("route").args(&["-n", "add", "-net", "10.33.0.0/16", &format!("{}", ip_addr)]).output().await;
                    match output {
                        Ok(output) => {
                            if !output.status.success() {
                                log::error!("add route error {}", String::from_utf8_lossy(&output.stderr));
                            } else {
                                log::info!("add route success");
                            }
                        }
                        Err(e) => {
                            log::error!("add route error {}", e);
                        }
                    }
                }

                let mut async_file = unsafe { File::from_raw_fd(dev.as_raw_fd()) };
                let mut buf = [0; 4096];

                loop {
                    select! {
                        e = async_file.read(&mut buf).fuse() => match e {
                            Ok(amount) => {
                                let to_ip = &buf[20..24];
                                let dest = NodeId::build(0, 0, to_ip[2], to_ip[3]);
                                if dest == ctx.node_id {
                                    log::debug!("write local tun {} bytes",  amount);
                                    dev.write(&buf[0..amount]).print_error("write tun error");
                                    continue;
                                } else {
                                    log::debug!("forward tun {} bytes to {}", amount, dest);
                                    let msg = TransportMsg::build_unreliable(TUNTAP_SERVICE_ID, RouteRule::ToNode(dest), 0, &buf[0..amount]);
                                    let mut actions = actions.write();
                                    actions.push_back(NetworkBehaviorAction::ToNet(msg));
                                    if actions.len() == 1 {
                                        ctx.awaker.notify();
                                    }
                                }
                            },
                            Err(e) => {
                                log::error!("read tun error {}", e);
                                break;
                            }
                        },
                        msg = rx.recv().fuse() => {
                            if let Ok(mut msg) = msg {
                                let payload = msg.payload_mut();
                                #[cfg(any(target_os = "macos", target_os = "ios"))]
                                {
                                    payload[2] = 0;
                                    payload[3] = 2;
                                }
                                #[cfg(any(target_os = "linux", target_os = "android"))]
                                {
                                    payload[2] = 8;
                                    payload[3] = 0;
                                }
                                log::debug!("write tun {} bytes", payload.len());
                                dev.write(payload).print_error("write tun error");
                            } else {
                                log::error!("read incoming msg error");
                                break;
                            }
                        }
                    };
                }
            });
            self.join = Some(join)
        }
    }

    fn on_tick(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _internal_ms: u64) {}

    fn check_incoming_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId, _local_uuid: TransportOutgoingLocalUuid) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_local_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _msg: TransportMsg) {
        panic!("Should not happend");
    }

    fn on_incoming_connection_connected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(TunTapHandler { local_tx: self.local_tx.clone() }))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        _ctx: &BehaviorContext,
        _now_ms: u64,
        _conn: Arc<dyn ConnectionSender>,
        _local_uuid: TransportOutgoingLocalUuid,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(TunTapHandler { local_tx: self.local_tx.clone() }))
    }

    fn on_incoming_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId) {}

    fn on_outgoing_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId) {}

    fn on_outgoing_connection_error(
        &mut self,
        _ctx: &BehaviorContext,
        _now_ms: u64,
        _node_id: NodeId,
        _conn_id: Option<ConnId>,
        _local_uuid: TransportOutgoingLocalUuid,
        _err: &OutgoingConnectionError,
    ) {
    }

    fn on_handler_event(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId, _event: BE) {}

    fn on_stopped(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}
}

impl<HE, SE> Drop for TunTapBehavior<HE, SE> {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            async_std::task::spawn(async move {
                join.cancel().await.print_none("Should cancel task");
            });
        }
    }
}
