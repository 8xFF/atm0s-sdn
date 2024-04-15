use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
    vec,
};

use atm0s_sdn::{
    base::ServiceBuilder,
    features::{FeaturesControl, FeaturesEvent},
    secure::{HandshakeBuilderXDA, StaticKeyAuthorization},
    services::visualization,
    ControllerPlaneCfg, DataPlaneCfg, DataWorkerHistory, NodeAddr, NodeAddrBuilder, NodeId, Protocol, SdnExtIn, SdnWorkerCfg,
};
use clap::Parser;
use sans_io_runtime::{backend::PollingBackend, Controller};

use worker::{ChannelId, Event, ExtIn, ExtOut, ICfg, SCfg, SC, SE, TC, TW};

use crate::worker::{ControllerCfg, RunnerOwner, RunnerWorker, SdnInnerCfg};

mod http;
mod sfu;
mod worker;

/// Quic-tunnel demo application
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Node Id
    #[arg(env, short, long, default_value_t = 1)]
    node_id: NodeId,

    /// Listen address
    #[arg(env, short, long, default_value_t = 10000)]
    udp_port: u16,

    /// Address of node we should connect to
    #[arg(env, short, long)]
    seeds: Vec<NodeAddr>,

    /// Password for the network
    #[arg(env, short, long, default_value = "password")]
    password: String,

    /// Workers
    #[arg(env, long, default_value_t = 1)]
    workers: usize,

    /// Http listen port
    #[arg(env, long, default_value_t = 8080)]
    http_port: u16,
}

fn main() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "info");
    }
    let args = Args::parse();
    env_logger::builder().format_timestamp_millis().init();

    let auth = Arc::new(StaticKeyAuthorization::new(&args.password));
    let history = Arc::new(DataWorkerHistory::default());

    let mut server = http::SimpleHttpServer::new(args.http_port);
    let mut controller = Controller::<ExtIn, ExtOut, SCfg, ChannelId, Event, 128>::default();
    let services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, SC, SE, TC, TW>>> = vec![Arc::new(visualization::VisualizationServiceBuilder::<SC, SE, TC, TW>::new(false))];

    let mut addr_builder = NodeAddrBuilder::new(args.node_id);
    addr_builder.add_protocol(Protocol::Ip4("192.168.1.27".parse().unwrap()));
    addr_builder.add_protocol(Protocol::Udp(args.udp_port));
    let addr = addr_builder.addr();
    log::info!("Node address: {}", addr);

    controller.add_worker::<RunnerOwner, _, RunnerWorker, PollingBackend<_, 128, 512>>(
        Duration::from_millis(10),
        ICfg {
            sfu: "192.168.1.27:0".parse().unwrap(),
            sdn: SdnInnerCfg {
                node_id: args.node_id,
                tick_ms: 1000,
                udp_port: args.udp_port,
                controller: Some(ControllerCfg {
                    session: 0,
                    auth,
                    handshake: Arc::new(HandshakeBuilderXDA),
                }),
                services: services.clone(),
                history: history.clone(),
                #[cfg(feature = "vpn")]
                vpn_tun_fd: None,
            },
            sdn_listen: SocketAddr::from(([0, 0, 0, 0], args.udp_port)),
        },
        None,
    );

    for _ in 1..args.workers {
        controller.add_worker::<RunnerOwner, _, RunnerWorker, PollingBackend<_, 128, 512>>(
            Duration::from_millis(10),
            ICfg {
                sfu: "192.168.1.27:0".parse().unwrap(),
                sdn: SdnInnerCfg {
                    node_id: args.node_id,
                    tick_ms: 1000,
                    udp_port: args.udp_port,
                    controller: None,
                    services: services.clone(),
                    history: history.clone(),
                    #[cfg(feature = "vpn")]
                    vpn_tun_fd: None,
                },
                sdn_listen: SocketAddr::from(([0, 0, 0, 0], args.udp_port)),
            },
            None,
        );
    }

    for seed in args.seeds {
        controller.send_to(0, ExtIn::Sdn(SdnExtIn::ConnectTo(seed)));
    }

    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term)).expect("Should register hook");

    while let Ok(req) = server.recv(Duration::from_millis(100)) {
        if controller.process().is_none() {
            break;
        }
        if term.load(Ordering::Relaxed) {
            controller.shutdown();
        }
        while let Some(ext) = controller.pop_event() {
            match ext {
                ExtOut::HttpResponse(resp) => {
                    server.send_response(resp);
                }
                ExtOut::Sdn(event) => {}
            }
        }
        if let Some(req) = req {
            controller.send_to_best(ExtIn::HttpRequest(req));
        }
    }

    log::info!("Server shutdown");
}
