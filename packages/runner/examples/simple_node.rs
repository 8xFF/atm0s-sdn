use atm0s_sdn_identity::{NodeAddr, NodeId};
use clap::Parser;
use std::time::Duration;

use atm0s_sdn::builder::SdnBuilder;

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
}

fn main() {
    let args = Args::parse();
    env_logger::builder().format_timestamp_millis().init();
    let mut builder = SdnBuilder::new(args.node_id, args.udp_port);

    for seed in args.seeds {
        builder.add_seed(seed);
    }

    let mut controller = builder.build(2);

    loop {
        controller.process();
        std::thread::sleep(Duration::from_millis(10));
    }
}
