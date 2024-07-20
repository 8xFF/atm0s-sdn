use atm0s_sdn::secure::StaticKeyAuthorization;
use atm0s_sdn::services::visualization;
use atm0s_sdn::{
    sans_io_runtime::backend::{PollBackend, PollingBackend},
    services::visualization::ConnectionInfo,
};
use atm0s_sdn::{NodeAddr, NodeId, SdnControllerUtils};
use atm0s_sdn::{SdnBuilder, SdnExtOut, SdnOwner};
use clap::{Parser, ValueEnum};
use futures_util::{SinkExt, StreamExt};
#[cfg(not(feature = "embed"))]
use poem::endpoint::StaticFilesEndpoint;
#[cfg(feature = "embed")]
use poem::endpoint::{EmbeddedFileEndpoint, EmbeddedFilesEndpoint};
use poem::{
    get, handler,
    listener::TcpListener,
    web::{
        websocket::{Message, WebSocket},
        Data,
    },
    EndpointExt, IntoResponse, Route, Server,
};
#[cfg(feature = "embed")]
use rust_embed::RustEmbed;

use serde::{Deserialize, Serialize};
use std::time::Instant;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::Mutex;

#[cfg(feature = "embed")]
#[derive(RustEmbed)]
#[folder = "public"]
pub struct Files;

#[derive(Debug, Clone, ValueEnum)]
enum BackendType {
    Poll,
    Polling,
}

/// Simple program to running a node
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Node Id
    #[arg(env, short, long, default_value_t = 0)]
    node_id: NodeId,

    /// Listen address
    #[arg(env, short, long, default_value = "127.0.0.1:10000")]
    bind_addr: SocketAddr,

    /// Address of node we should connect to
    #[arg(env, short, long)]
    seeds: Vec<NodeAddr>,

    /// Password for the network
    #[arg(env, short, long, default_value = "password")]
    password: String,

    /// Backend type
    #[arg(env, short, long, default_value = "polling")]
    backend: BackendType,

    /// Enable Tun-Tap interface for create a vpn network between nodes
    #[arg(env, short, long)]
    vpn: bool,

    /// Workers
    #[arg(env, long, default_value_t = 2)]
    workers: usize,

    /// Custom IP
    #[arg(env, long)]
    custom_addrs: Vec<SocketAddr>,

    /// Local tags
    #[arg(env, long)]
    local_tags: Vec<String>,

    /// Connect tags
    #[arg(env, long)]
    connect_tags: Vec<String>,

    /// Web server addr
    #[arg(env, long, default_value = "0.0.0.0:3000")]
    web_addr: SocketAddr,

    /// Collector node, which will have UI for monitoring network structure
    #[arg(env, long)]
    collector: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VisualNodeInfo {
    uptime: u32,
}
type SC = visualization::Control<VisualNodeInfo>;
type SE = visualization::Event<VisualNodeInfo>;
type TC = ();
type TW = ();

#[derive(Debug, Clone, Serialize)]
struct ConnInfo {
    uuid: u64,
    outgoing: bool,
    dest: NodeId,
    remote: SocketAddr,
    rtt_ms: u32,
}

#[derive(Debug, Clone, Serialize)]
struct NodeInfo {
    id: NodeId,
    info: VisualNodeInfo,
    connections: Vec<ConnInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
enum WebsocketMessage {
    Snapshot(Vec<NodeInfo>),
    Update(NodeInfo),
    Delete(NodeId),
}

#[derive(Debug)]
struct WebsocketCtx {
    channel: tokio::sync::broadcast::Sender<WebsocketMessage>,
    snapshot: HashMap<NodeId, (VisualNodeInfo, Vec<ConnInfo>)>,
}

impl WebsocketCtx {
    pub fn new() -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(10);
        Self {
            channel: tx,
            snapshot: HashMap::new(),
        }
    }

    pub fn set_snapshot(&mut self, snapshot: Vec<(NodeId, VisualNodeInfo, Vec<ConnectionInfo>)>) {
        self.snapshot.clear();
        for (id, info, connections) in snapshot {
            let conns = connections
                .into_iter()
                .map(|c| ConnInfo {
                    uuid: c.conn.session(),
                    outgoing: c.conn.is_outgoing(),
                    dest: c.dest,
                    remote: c.remote,
                    rtt_ms: c.rtt_ms,
                })
                .collect();
            self.snapshot.insert(id, (info, conns));
        }
    }

    pub fn set_node(&mut self, delta: (NodeId, VisualNodeInfo, Vec<ConnectionInfo>)) {
        let connections: Vec<ConnInfo> = delta
            .2
            .into_iter()
            .map(|c| ConnInfo {
                uuid: c.conn.session(),
                outgoing: c.conn.is_outgoing(),
                dest: c.dest,
                remote: c.remote,
                rtt_ms: c.rtt_ms,
            })
            .collect();
        self.snapshot.insert(delta.0, (delta.1.clone(), connections.clone()));
        if let Err(e) = self.channel.send(WebsocketMessage::Update(NodeInfo {
            id: delta.0,
            info: delta.1,
            connections,
        })) {
            log::debug!("Failed to send delta: {}", e);
        }
    }

    pub fn del_node(&mut self, id: NodeId) {
        self.snapshot.remove(&id);
        if let Err(e) = self.channel.send(WebsocketMessage::Delete(id)) {
            log::error!("Failed to send delta: {}", e);
        }
    }

    pub fn get_snapshot(&self) -> Vec<NodeInfo> {
        self.snapshot
            .iter()
            .map(|(id, (info, connections))| NodeInfo {
                id: *id,
                info: info.clone(),
                connections: connections.clone(),
            })
            .collect()
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<WebsocketMessage> {
        self.channel.subscribe()
    }
}

#[handler]
fn ws(ws: WebSocket, ctx: Data<&Arc<Mutex<WebsocketCtx>>>) -> impl IntoResponse {
    let ctx = ctx.clone();
    ws.on_upgrade(move |socket| async move {
        let (mut sink, mut _stream) = socket.split();
        let ctx = ctx.lock().await;
        let snapshot = ctx.get_snapshot();
        if let Err(e) = sink
            .send(Message::Text(serde_json::to_string(&WebsocketMessage::Snapshot(snapshot)).expect("should convert json")))
            .await
        {
            log::error!("Failed to send snapshot: {}", e);
            return;
        }
        let mut rx = ctx.subscribe();
        drop(ctx);
        while let Ok(event) = rx.recv().await {
            if let Err(e) = sink.send(Message::Text(serde_json::to_string(&event).expect("should convert json"))).await {
                log::error!("Failed to send delta: {}", e);
                return;
            }
        }
    })
}

#[tokio::main]
async fn main() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "info");
    }
    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term)).expect("Should register hook");
    let mut shutdown_wait = 0;
    let args = Args::parse();
    tracing_subscriber::fmt::init();
    let mut builder = SdnBuilder::<(), SC, SE, TC, TW, VisualNodeInfo>::new(args.node_id, args.bind_addr, args.custom_addrs);

    builder.set_authorization(StaticKeyAuthorization::new(&args.password));
    builder.set_manual_discovery(args.local_tags, args.connect_tags);

    if args.vpn {
        builder.enable_vpn();
    }

    builder.set_visualization_collector(args.collector);

    for seed in args.seeds {
        builder.add_seed(seed);
    }

    let node_info = VisualNodeInfo { uptime: 0 };
    let mut controller = match args.backend {
        BackendType::Poll => builder.build::<PollBackend<SdnOwner, 128, 128>>(args.workers, node_info),
        BackendType::Polling => builder.build::<PollingBackend<SdnOwner, 128, 128>>(args.workers, node_info),
    };

    let ctx = Arc::new(Mutex::new(WebsocketCtx::new()));

    if args.collector {
        controller.service_control(visualization::SERVICE_ID.into(), (), visualization::Control::Subscribe);
        let ctx_c = ctx.clone();
        tokio::spawn(async move {
            let route = Route::new().at("/ws", get(ws.data(ctx_c)));

            #[cfg(not(feature = "embed"))]
            let route = route.nest("/", StaticFilesEndpoint::new("./public/").index_file("index.html"));

            #[cfg(feature = "embed")]
            let route = route.at("/", EmbeddedFileEndpoint::<Files>::new("index.html"));
            #[cfg(feature = "embed")]
            let route = route.nest("/", EmbeddedFilesEndpoint::<Files>::new());

            Server::new(TcpListener::bind(args.web_addr)).run(route).await
        });
    }

    let started_at = Instant::now();
    let mut count = 0;
    while controller.process().is_some() {
        if term.load(Ordering::Relaxed) {
            if shutdown_wait == 200 {
                log::warn!("Force shutdown");
                break;
            }
            shutdown_wait += 1;
            controller.shutdown();
        }
        if count % 500 == 0 {
            //each 5 seconds
            controller.service_control(
                visualization::SERVICE_ID.into(),
                (),
                visualization::Control::UpdateInfo(VisualNodeInfo {
                    uptime: started_at.elapsed().as_secs() as u32,
                }),
            );
        }
        while let Some(event) = controller.pop_event() {
            match event {
                SdnExtOut::ServicesEvent(_service, (), event) => match event {
                    visualization::Event::GotAll(all) => {
                        log::info!("Got all: {:?}", all);
                        ctx.lock().await.set_snapshot(all);
                    }
                    visualization::Event::NodeChanged(node, info, changed) => {
                        log::debug!("Node changed: {:?} {:?}", node, changed);
                        ctx.lock().await.set_node((node, info, changed));
                    }
                    visualization::Event::NodeRemoved(node) => {
                        log::info!("Node removed: {:?}", node);
                        ctx.lock().await.del_node(node);
                    }
                },
                SdnExtOut::FeaturesEvent(_, _) => {}
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        count += 1;
    }

    log::info!("Server shutdown");
}
