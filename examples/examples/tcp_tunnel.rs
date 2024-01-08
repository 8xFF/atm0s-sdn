use async_std::net::TcpListener;
use async_std::net::TcpStream;
use atm0s_sdn::convert_enum;
use atm0s_sdn::virtual_socket::create_vnet;
use atm0s_sdn::virtual_socket::make_insecure_quinn_client;
use atm0s_sdn::virtual_socket::make_insecure_quinn_server;
use atm0s_sdn::virtual_socket::quinn::Connection;
use atm0s_sdn::virtual_socket::quinn::Endpoint;
use atm0s_sdn::virtual_socket::quinn::RecvStream;
use atm0s_sdn::virtual_socket::quinn::SendStream;
use atm0s_sdn::virtual_socket::vnet_addr;
use atm0s_sdn::virtual_socket::VirtualNet;
use atm0s_sdn::NodeId;
use atm0s_sdn::SharedRouter;
use atm0s_sdn::SystemTimer;
use atm0s_sdn::{KeyValueBehavior, NodeAddr, NodeAddrBuilder, UdpTransport};
use atm0s_sdn::{KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueSdkEvent};
use atm0s_sdn::{LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent};
use atm0s_sdn::{ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent};
use atm0s_sdn::{NetworkPlane, NetworkPlaneConfig};
use clap::Parser;
use clap::Subcommand;
use std::error::Error;
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
}

#[derive(Parser, Debug, Clone)]
struct AgentOpts {
    /// Tunnel Agent target addr, which will be forwarded to
    #[arg(env, long)]
    target: SocketAddr,
}

async fn open_tunnel_to(client: &Endpoint, addr: SocketAddr) -> Result<(Connection, SendStream, RecvStream), Box<dyn Error + Send + Sync>> {
    let connection = client.connect(addr, "localhost").unwrap().await?;
    let (send, recv) = connection.open_bi().await?;
    Ok((connection, send, recv))
}

async fn run_server(sdk: VirtualNet, opts: ServerOpts) {
    let listener = TcpListener::bind(opts.listen).await.expect("Should bind");
    while let Ok((stream, remote_addr)) = listener.accept().await {
        log::info!("[TcpTunnel][Server] incomming conn from {}", remote_addr);
        let client = make_insecure_quinn_client(sdk.create_udp_socket(0, 100).unwrap()).unwrap();
        let client_local = client.local_addr().expect("");
        async_std::task::spawn(async move {
            log::info!("[TcpTunnel][Server] connecting to dest node {}", opts.dest);
            match open_tunnel_to(&client, vnet_addr(opts.dest, 80)).await {
                Ok((connection, send, mut recv)) => {
                    let vnet_dest = connection.remote_address();
                    log::info!("[TcpTunnel][Server] connected to dest node, pipe {} <==> {} <==> {}", remote_addr, client_local, vnet_dest);
                    let stream_c = stream.clone();
                    let task1 = async_std::task::spawn(async move {
                        match async_std::io::copy(stream, send).await {
                            Ok(_) => {}
                            Err(e) => {
                                log::info!("[TcpTunnel][Server] copy to dest {} ==> {} ==> {} error {:?}", remote_addr, client_local, vnet_dest, e);
                            }
                        }
                    });
                    let task2 = async_std::task::spawn(async move {
                        match async_std::io::copy(&mut recv, stream_c).await {
                            Ok(_) => {}
                            Err(e) => {
                                log::info!("[TcpTunnel][Server] copy from dest {} ==> {} ==> {} error {:?}", vnet_dest, client_local, remote_addr, e);
                            }
                        }
                    });
                    task1.await;
                    task2.await;
                    log::info!("[TcpTunnel][Server] disconnected pipe {} <==> {} <==> {}", remote_addr, client_local, vnet_dest);
                }
                Err(e) => {
                    log::info!("[TcpTunnel][Server] connect to dest node {} errpr {:?}", opts.dest, e);
                }
            }
        });
    }
}

async fn run_agent(sdk: VirtualNet, opts: AgentOpts) {
    let endpoint = make_insecure_quinn_server(sdk.create_udp_socket(80, 200).expect("")).expect("");
    while let Some(connecting) = endpoint.accept().await {
        log::info!("[TcpTunnel][Agent] incomming conn from {:?}", connecting.remote_address());
        async_std::task::spawn(async move {
            let connection = connecting.await.expect("Should accept");
            let (send, recv) = connection.accept_bi().await.expect("Should open bi");
            let vnet_remote = connection.remote_address();
            log::info!("[TcpTunnel][Agent] vnet incomming {} connecting to target {}", vnet_remote, opts.target);
            match TcpStream::connect(&opts.target).await {
                Ok(target) => {
                    let local_addr = target.local_addr().expect("");
                    log::info!("[TcpTunnel][Agent] connected to target, pipe {} <==> {} <==> {}", opts.target, local_addr, vnet_remote);
                    let target_c = target.clone();
                    let task1 = async_std::task::spawn(async move {
                        match async_std::io::copy(target, send).await {
                            Ok(_) => {}
                            Err(e) => {
                                log::info!("[TcpTunnel][Agent] copy from target {} ==> {} ==> {} error {:?}", opts.target, local_addr, vnet_remote, e);
                            }
                        }
                    });
                    let task2 = async_std::task::spawn(async move {
                        match async_std::io::copy(recv, target_c).await {
                            Ok(_) => {}
                            Err(e) => {
                                log::info!("[TcpTunnel][Agent] copy to target {} ==> {} ==> {} error {:?}", vnet_remote, local_addr, opts.target, e);
                            }
                        }
                    });
                    task1.await;
                    task2.await;
                    log::info!("[TcpTunnel][Agent] disconnected {} <==> {} <==> {}", opts.target, local_addr, vnet_remote);
                }
                Err(e) => {
                    log::info!("[TcpTunnel][Agent] connect to target {} error {:?}", opts.target, e);
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

    let router = Arc::new(router);
    let (virtual_socket, virtual_socket_sdk) = create_vnet(args.node_id, router.clone());

    let plan_cfg = NetworkPlaneConfig {
        router,
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
        }
        Mode::Agent(opts) => {
            let sdk_c = virtual_socket_sdk.clone();
            let opts_c = opts.clone();
            async_std::task::spawn(async move {
                run_agent(sdk_c, opts_c).await;
            });
        }
    }

    while let Ok(_) = plane.recv().await {}

    plane.stopped();
}
