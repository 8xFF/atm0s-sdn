use async_std::io::ReadExt;
use async_std::io::WriteExt;
use async_std::net::TcpListener;
use async_std::net::TcpStream;
use async_std::net::UdpSocket;
use atm0s_sdn::compose_transport_desp::select;
use atm0s_sdn::compose_transport_desp::FutureExt;
use atm0s_sdn::convert_enum;
use atm0s_sdn::NodeId;
use atm0s_sdn::SharedRouter;
use atm0s_sdn::SystemTimer;
use atm0s_sdn::VirtualSocketBehavior;
use atm0s_sdn::VirtualSocketSdk;
use atm0s_sdn::VirtualStream;
use atm0s_sdn::{KeyValueBehavior, NodeAddr, NodeAddrBuilder, UdpTransport};
use atm0s_sdn::{KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueSdkEvent};
use atm0s_sdn::{LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent};
use atm0s_sdn::{ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent};
use atm0s_sdn::{NetworkPlane, NetworkPlaneConfig};
use clap::Parser;
use clap::Subcommand;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeBehaviorEvent {
    Manual(ManualBehaviorEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncBehaviorEvent),
    KeyValue(KeyValueBehaviorEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeHandleEvent {
    Manual(ManualHandlerEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncHandlerEvent),
    KeyValue(KeyValueHandlerEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeSdkEvent {
    KeyValue(KeyValueSdkEvent),
}

/// Node with manual network builder
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Current Node ID
    #[arg(env, long)]
    node_id: u32,

    /// Neighbours
    #[arg(env, long)]
    seeds: Vec<NodeAddr>,

    /// Local tags
    #[arg(env, long)]
    tags: Vec<String>,

    /// Tags of nodes to connect
    #[arg(env, long)]
    connect_tags: Vec<String>,

    ///
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Debug, Subcommand)]
enum Mode {
    Server(ServerOpts),
    Agent(AgentOpts),
}

#[derive(Parser, Debug, Clone)]
struct ServerOpts {
    /// Tunnel Server listen TCP port
    #[arg(env, long)]
    listen: SocketAddr,

    /// Tunnel dest node_id
    #[arg(env, long)]
    dest: NodeId,

    /// Tunnel is encrypted or not
    #[arg(env, long)]
    secure: bool,
}

#[derive(Parser, Debug, Clone)]
struct AgentOpts {
    /// Tunnel Agent target addr, which will be forwarded to
    #[arg(env, long)]
    target: SocketAddr,
}

async fn run_server(sdk: VirtualSocketSdk, opts: ServerOpts) {
    let listener = TcpListener::bind(opts.listen).await.expect("Should bind");
    while let Ok((mut stream, remote_addr)) = listener.accept().await {
        log::info!("[TcpTunnel][Server] incomming conn from {}", remote_addr);
        let connector = sdk.connector();
        async_std::task::spawn(async move {
            log::info!("[TcpTunnel][Server] connecting to dest node {}", opts.dest);
            match connector.connect_to(opts.secure, opts.dest, "TUNNEL_APP", HashMap::new()).await {
                Ok(socket_relay) => {
                    log::info!("[TcpTunnel][Server] connected to dest node {} remote {:?}", opts.dest, socket_relay.remote());
                    let mut target = VirtualStream::new(socket_relay);
                    let mut buf1 = [0; 4096];
                    let mut buf2 = [0; 4096];
                    loop {
                        select! {
                            e = stream.read(&mut buf1).fuse() => {
                                if let Ok(len) = e {
                                    if len == 0 {
                                        break;
                                    }
                                    target.write_all(&buf1[..len]).await.expect("Should write");
                                } else {
                                    break;
                                }
                            },
                            e = target.read(&mut buf2).fuse() => {
                                if let Ok(len) = e {
                                    if len == 0 {
                                        break;
                                    }
                                    stream.write_all(&buf2[..len]).await.expect("Should write");
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::info!("[TcpTunnel][Server] connect to dest node {} errpr {:?}", opts.dest, e);
                }
            }
        });
    }
}

async fn run_agent(sdk: VirtualSocketSdk, opts: AgentOpts) {
    let mut listener = sdk.listen("TUNNEL_APP");
    while let Some(socket) = listener.recv().await {
        log::info!("[TcpTunnel][Agent] incomming conn from {:?}", socket.remote());
        let mut stream = VirtualStream::new(socket);
        async_std::task::spawn(async move {
            log::info!("[TcpTunnel][Agent] connecting to target {}", opts.target);
            match TcpStream::connect(&opts.target).await {
                Ok(mut target) => {
                    log::info!("[TcpTunnel][Agent] connected to target {}", opts.target);
                    let mut buf1 = [0; 4096];
                    let mut buf2 = [0; 4096];
                    loop {
                        select! {
                            e = stream.read(&mut buf1).fuse() => {
                                if let Ok(len) = e {
                                    if len == 0 {
                                        break;
                                    }
                                    target.write_all(&buf1[..len]).await.expect("Should write");
                                } else {
                                    break;
                                }
                            },
                            e = target.read(&mut buf2).fuse() => {
                                if let Ok(len) = e {
                                    if len == 0 {
                                        break;
                                    }
                                    stream.write_all(&buf2[..len]).await.expect("Should write");
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::info!("[TcpTunnel][Agent] connect to target {} error {:?}", opts.target, e);
                }
            }
        });
    }
}

async fn run_udp_server(sdk: VirtualSocketSdk, opts: ServerOpts) {
    let udp_server = UdpSocket::bind(opts.listen).await.expect("Should bind");
    log::info!("[UdpTunnel][Server] listen on {}", opts.listen);
    let mut buf1 = [0; 1500];
    let (_, remote_addr) = udp_server.peek_from(&mut buf1).await.expect("Should peek");
    udp_server.connect(remote_addr).await.expect("Should connect");
    log::info!("[UdpTunnel][Server] incomming conn from {}", remote_addr);
    log::info!("[UdpTunnel][Server] connecting to dest node {}", opts.dest);
    match sdk.connector().connect_to(opts.secure, opts.dest, "TUNNEL_APP_UDP", HashMap::new()).await {
        Ok(mut socket_relay) => {
            log::info!("[UdpTunnel][Server] connected to dest node {} remote {:?}", opts.dest, socket_relay.remote());
            loop {
                select! {
                    e = udp_server.recv(&mut buf1).fuse() => {
                        if let Ok(len) = e {
                            if len == 0 {
                                break;
                            }
                            socket_relay.write(&buf1[..len]).expect("Should write");
                        } else {
                            break;
                        }
                    },
                    e = socket_relay.read().fuse() => {
                        if let Some(buf) = e {
                            udp_server.send(&buf).await.expect("Should write");
                        } else {
                            break;
                        }
                    }
                }
            }
        }
        Err(e) => {
            log::info!("[UdpTunnel][Server] connect to dest node {} errpr {:?}", opts.dest, e);
        }
    }
}

async fn run_udp_agent(sdk: VirtualSocketSdk, opts: AgentOpts) {
    let mut listener = sdk.listen("TUNNEL_APP_UDP");
    while let Some(mut socket) = listener.recv().await {
        log::info!("[UdpTunnel][Agent] incomming conn from {:?}", socket.remote());
        async_std::task::spawn(async move {
            log::info!("[UdpTunnel][Agent] connecting to target {}", opts.target);
            let udp_socket = UdpSocket::bind("0.0.0.0:0").await.expect("Should bind");
            match udp_socket.connect(&opts.target).await {
                Ok(_) => {
                    log::info!("[UdpTunnel][Agent] connected to target {}", opts.target);
                    let mut buf2 = [0; 1500];
                    loop {
                        select! {
                            e = socket.read().fuse() => {
                                if let Some(buf) = e {
                                    udp_socket.send(&buf).await;
                                } else {
                                    break;
                                }
                            },
                            e = udp_socket.recv(&mut buf2).fuse() => {
                                if let Ok(len) = e {
                                    if len == 0 {
                                        break;
                                    }
                                    socket.write(&buf2[..len]).expect("Should write");
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::info!("[UdpTunnel][Agent] connect to target {} error {:?}", opts.target, e);
                }
            }
        });
    }
}

#[async_std::main]
async fn main() {
    env_logger::builder().format_timestamp_millis().init();
    let args: Args = Args::parse();
    let secure = Arc::new(atm0s_sdn::StaticKeySecure::new("secure-token"));
    let mut node_addr_builder = NodeAddrBuilder::new(args.node_id);

    let udp_socket = UdpTransport::prepare(50000 + args.node_id as u16, &mut node_addr_builder).await;
    let transport = UdpTransport::new(node_addr_builder.addr(), udp_socket, secure.clone());

    let node_addr = node_addr_builder.addr();
    log::info!("Listen on addr {}", node_addr);

    let timer = Arc::new(SystemTimer());
    let router = SharedRouter::new(args.node_id);

    let manual = ManualBehavior::new(ManualBehaviorConf {
        node_id: args.node_id,
        node_addr,
        seeds: args.seeds.clone(),
        local_tags: args.tags,
        connect_tags: args.connect_tags,
    });

    let spreads_layer_router: LayersSpreadRouterSyncBehavior = LayersSpreadRouterSyncBehavior::new(router.clone());
    let key_value = KeyValueBehavior::new(args.node_id, 10000, None);

    let (virtual_socket, virtual_socket_sdk) = VirtualSocketBehavior::new(args.node_id);

    let plan_cfg = NetworkPlaneConfig {
        router: Arc::new(router),
        node_id: args.node_id,
        tick_ms: 1000,
        behaviors: vec![Box::new(manual), Box::new(spreads_layer_router), Box::new(key_value), Box::new(virtual_socket)],
        transport: Box::new(transport),
        timer,
    };

    let mut plane = NetworkPlane::<NodeBehaviorEvent, NodeHandleEvent, NodeSdkEvent>::new(plan_cfg);

    plane.started();

    match args.mode {
        Mode::Server(opts) => {
            let sdk_c = virtual_socket_sdk.clone();
            let opts_c = opts.clone();
            async_std::task::spawn(async move {
                run_server(sdk_c, opts_c).await;
            });
            let sdk_c = virtual_socket_sdk.clone();
            let opts_c = opts.clone();
            async_std::task::spawn(async move {
                run_udp_server(sdk_c, opts_c).await;
            });
        }
        Mode::Agent(opts) => {
            let sdk_c = virtual_socket_sdk.clone();
            let opts_c = opts.clone();
            async_std::task::spawn(async move {
                run_agent(sdk_c, opts_c).await;
            });
            let sdk_c = virtual_socket_sdk.clone();
            let opts_c = opts.clone();
            async_std::task::spawn(async move {
                run_udp_agent(sdk_c, opts_c).await;
            });
        }
    }

    while let Ok(_) = plane.recv().await {}

    plane.stopped();
}
