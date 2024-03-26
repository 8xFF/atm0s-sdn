use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use atm0s_sdn::{
    builder::SdnBuilder,
    features::{socket, FeaturesControl, FeaturesEvent},
    sans_io_runtime::{backend::PollBackend, Owner},
    services::visualization,
    tasks::{SdnExtIn, SdnExtOut},
    NodeAddr, NodeId,
};
use tokio::sync::mpsc::{Receiver, Sender};

use crate::vnet::{NetworkPkt, OutEvent};

type SC = visualization::Control;
type SE = visualization::Event;
type TC = ();
type TW = ();

pub async fn run_sdn(node_id: NodeId, udp_port: u16, seeds: Vec<NodeAddr>, workers: usize, tx: Sender<NetworkPkt>, mut rx: Receiver<OutEvent>) {
    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term)).expect("Should register hook");
    let mut shutdown_wait = 0;
    let mut builder = SdnBuilder::<SC, SE, TC, TW>::new(node_id, udp_port, vec![]);

    builder.set_manual_discovery(vec!["tunnel".to_string()], vec!["tunnel".to_string()]);
    builder.set_visualization_collector(false);

    for seed in seeds {
        builder.add_seed(seed);
    }

    let mut controller = builder.build::<PollBackend<128, 128>>(workers);
    while controller.process().is_some() {
        if term.load(Ordering::Relaxed) {
            if shutdown_wait == 200 {
                log::warn!("Force shutdown");
                break;
            }
            shutdown_wait += 1;
            controller.shutdown();
        }
        while let Ok(c) = rx.try_recv() {
            // log::info!("Command: {:?}", c);
            match c {
                OutEvent::Bind(port) => {
                    controller.send_to(Owner::worker(0), SdnExtIn::FeaturesControl(FeaturesControl::Socket(socket::Control::Bind(port))));
                }
                OutEvent::Pkt(pkt) => {
                    let send = socket::Control::SendTo(pkt.local_port, pkt.remote, pkt.remote_port, pkt.data, pkt.meta);
                    controller.send_to(Owner::worker(0), SdnExtIn::FeaturesControl(FeaturesControl::Socket(send)));
                }
                OutEvent::Unbind(port) => {
                    controller.send_to(Owner::worker(0), SdnExtIn::FeaturesControl(FeaturesControl::Socket(socket::Control::Unbind(port))));
                }
            }
        }
        while let Some(event) = controller.pop_event() {
            // log::info!("Event: {:?}", event);
            match event {
                SdnExtOut::FeaturesEvent(FeaturesEvent::Socket(socket::Event::RecvFrom(local_port, remote, remote_port, data, meta))) => {
                    if let Err(e) = tx.try_send(NetworkPkt {
                        local_port,
                        remote,
                        remote_port,
                        data,
                        meta,
                    }) {
                        log::error!("Failed to send to tx: {:?}", e);
                    }
                }
                _ => {}
            }
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}
