use std::{
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
    tasks::{ControllerCfg, DataWorkerHistory, SdnExtIn},
    NodeAddr, NodeId,
};
use clap::Parser;
use sans_io_runtime::{backend::PollingBackend, Controller};

use worker::{ChannelId, Event, ExtIn, ExtOut, ICfg, SCfg, SC, SE, TC, TW};

use crate::worker::{RunnerOwner, RunnerWorker};

mod http;
mod sfu;
mod worker;

/// Quic-tunnel demo application
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Node Id
    #[arg(env, short, long, default_value_t = 0)]
    node_id: NodeId,

    /// Listen address
    #[arg(env, short, long, default_value_t = 0)]
    udp_port: u16,

    /// Address of node we should connect to
    #[arg(env, short, long)]
    seeds: Vec<NodeAddr>,

    /// Password for the network
    #[arg(env, short, long, default_value = "password")]
    password: String,

    /// Workers
    #[arg(env, long, default_value_t = 2)]
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

    let handshake = StaticKeyAuthorization::new(&args.password);
    let history = Arc::new(DataWorkerHistory::default());

    let mut server = http::SimpleHttpServer::new(args.http_port);
    let mut controller = Controller::<ExtIn, ExtOut, SCfg, ChannelId, Event, 128>::default();
    let services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, SC, SE, TC, TW>>> = vec![Arc::new(visualization::VisualizationServiceBuilder::<SC, SE, TC, TW>::new(false))];

    controller.add_worker::<RunnerOwner, _, RunnerWorker, PollingBackend<_, 128, 512>>(
        Duration::from_millis(1),
        ICfg {
            sfu: sfu::ICfg {
                udp_addr: "192.168.1.39:0".parse().unwrap(),
            },
            sdn: atm0s_sdn::tasks::SdnInnerCfg {
                node_id: args.node_id,
                udp_port: args.udp_port,
                controller: Some(ControllerCfg {
                    session: 0,
                    auth: Arc::new(handshake),
                    handshake: Arc::new(HandshakeBuilderXDA),
                    tick_ms: 1000,
                }),
                services: services.clone(),
                history: history.clone(),
            },
        },
        None,
    );

    for _ in 1..args.workers {
        controller.add_worker::<RunnerOwner, _, RunnerWorker, PollingBackend<_, 128, 512>>(
            Duration::from_millis(1),
            ICfg {
                sfu: sfu::ICfg {
                    udp_addr: "192.168.1.39:0".parse().unwrap(),
                },
                sdn: atm0s_sdn::tasks::SdnInnerCfg {
                    node_id: args.node_id,
                    udp_port: args.udp_port,
                    controller: None,
                    services: services.clone(),
                    history: history.clone(),
                },
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
                ExtOut::Sfu(sfu::ExtOut::HttpResponse(resp)) => {
                    server.send_response(resp);
                }
                ExtOut::Sdn(event) => {
                    log::info!("SDN event: {:?}", event);
                }
            }
        }
        if let Some(req) = req {
            controller.spawn(SCfg::Sfu(sfu::SCfg::HttpRequest(req)));
        }
    }

    log::info!("Server shutdown");
}
