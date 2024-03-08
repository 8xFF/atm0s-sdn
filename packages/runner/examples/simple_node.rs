use clap::Parser;
use std::{net::SocketAddr, time::Duration};

use atm0s_sdn::tasks::*;
use sans_io_runtime::backend::PollBackend;

/// Simple program to running a node
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Node Id
    #[arg(short, long)]
    node_id: u32,

    /// Listen address
    #[arg(short, long)]
    udp_port: u16,

    /// Address of node we should connect to
    #[arg(short, long)]
    seeds: Vec<SocketAddr>,

    /// Password for the network
    #[arg(short, long, default_value = "password")]
    password: String,
}

fn main() {
    let args = Args::parse();
    env_logger::builder().format_timestamp_millis().init();
    let mut controler = SdnController::default();
    controler.add_worker::<_, SdnWorkerInner, PollBackend<128, 128>>(
        SdnInnerCfg {
            node_id: args.node_id,
            udp_port: args.udp_port,
            controller: Some(ControllerCfg {
                password: "password".to_string(),
                behaviours: vec![],
                seeds: args.seeds,
            }),
        },
        None,
    );

    controler.add_worker::<_, SdnWorkerInner, PollBackend<128, 128>>(
        SdnInnerCfg {
            node_id: args.node_id,
            udp_port: args.udp_port,
            controller: None,
        },
        None,
    );

    loop {
        controler.process();
        std::thread::sleep(Duration::from_millis(10));
    }
}
