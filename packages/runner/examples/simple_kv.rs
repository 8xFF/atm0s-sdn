use atm0s_sdn_identity::{NodeAddr, NodeId};
use atm0s_sdn_network::features::{
    dht_kv::{Control, Key, Map, MapControl},
    FeaturesControl,
};
use clap::{Parser, ValueEnum};
use sans_io_runtime::{
    backend::{MioBackend, PollBackend, PollingBackend},
    Owner,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use atm0s_sdn::{builder::SdnBuilder, tasks::SdnExtIn};

#[derive(Debug, Clone, ValueEnum)]
enum BackendType {
    Poll,
    Polling,
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
    #[arg(short, long, default_value = "polling")]
    backend: BackendType,

    /// Workers
    #[arg(short, long, default_value_t = 1)]
    workers: usize,
}

type TC = ();
type TW = ();

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
        BackendType::Mio => builder.build::<MioBackend<128, 128>, TC, TW>(args.workers),
        BackendType::Poll => builder.build::<PollBackend<128, 128>, TC, TW>(args.workers),
        BackendType::Polling => builder.build::<PollingBackend<128, 128>, TC, TW>(args.workers),
    };

    controller.send_to(Owner::worker(0), SdnExtIn::FeaturesControl(FeaturesControl::DhtKv(Control::MapCmd(Map(1000), MapControl::Sub))));

    let mut i: u32 = 0;
    while controller.process().is_some() {
        if term.load(Ordering::Relaxed) {
            if shutdown_wait == 200 {
                log::warn!("Force shutdown");
                break;
            }
            shutdown_wait += 1;
            controller.shutdown();
        }
        std::thread::sleep(Duration::from_millis(1));
        i += 1;

        if i % 1000 == 0 {
            let data = i.to_be_bytes().to_vec();
            log::info!("Set key: {:?}", data);
            controller.send_to(
                Owner::worker(0),
                SdnExtIn::FeaturesControl(FeaturesControl::DhtKv(Control::MapCmd(Map(1000), MapControl::Set(Key(200), data)))),
            );
        }
        while let Some(out) = controller.pop_event() {
            log::info!("Got event: {:?}", out);
        }
    }

    log::info!("Server shutdown");
}
