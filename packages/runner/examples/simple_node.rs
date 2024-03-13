use atm0s_sdn_identity::{NodeAddr, NodeId};
use clap::{Parser, ValueEnum};
use sans_io_runtime::backend::{MioBackend, PollBackend};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use atm0s_sdn::builder::SdnBuilder;

#[derive(Debug, Clone, ValueEnum)]
enum BackendType {
    Poll,
    Mio,
}

/// Simple program to running a node
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Node Id
    #[arg(short, long)]
    node_id: NodeId,

    /// Listen address
    #[arg(short, long)]
    udp_port: u16,

    /// Address of node we should connect to
    #[arg(short, long)]
    seeds: Vec<NodeAddr>,

    /// Password for the network
    #[arg(short, long, default_value = "password")]
    password: String,

    /// Backend type
    #[arg(short, long, default_value = "poll")]
    backend: BackendType,
}

fn main() {
    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term)).expect("Should register hook");
    let mut shutdown_wait = 0;
    let args = Args::parse();
    env_logger::builder().format_timestamp_millis().init();
    let mut builder = SdnBuilder::new(args.node_id, args.udp_port);

    for seed in args.seeds {
        builder.add_seed(seed);
    }

    let mut controller = match args.backend {
        BackendType::Mio => builder.build::<MioBackend<128, 128>>(2),
        BackendType::Poll => builder.build::<PollBackend<128, 128>>(2),
    };

    while controller.process().is_some() {
        if term.load(Ordering::Relaxed) {
            if shutdown_wait == 200 {
                log::warn!("Force shutdown");
                break;
            }
            shutdown_wait += 1;
            controller.shutdown();
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    log::info!("Server shutdown");
}
