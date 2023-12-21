use std::time::Duration;

use atm0s_sdn::RouteRule;
use atm0s_sdn::{ConnectionEvent, Transport, TransportEvent, TransportMsg};
use atm0s_sdn::{NodeAddrBuilder, UdpTransport};

#[async_std::main]
async fn main() {
    env_logger::builder().format_timestamp_millis().filter_level(log::LevelFilter::Info).init();
    let mut node_addr_builder1 = NodeAddrBuilder::new(1);
    let socket1 = UdpTransport::prepare(0, &mut node_addr_builder1).await;
    let mut transport1 = Box::new(UdpTransport::new(node_addr_builder1.addr(), socket1));

    let mut node_addr_builder2 = NodeAddrBuilder::new(2);
    let socket2 = UdpTransport::prepare(0, &mut node_addr_builder2).await;
    let mut transport2 = Box::new(UdpTransport::new(node_addr_builder2.addr(), socket2));

    let task = async_std::task::spawn(async move {
        loop {
            match transport2.recv().await {
                Ok(TransportEvent::IncomingRequest(_, _, acceptor)) => {
                    log::info!("[Transport2] IncomingRequest");
                    acceptor.accept();
                }
                Ok(TransportEvent::Incoming(trans2_sender, mut trans2_receiver)) => {
                    while let Ok(msg) = trans2_receiver.poll().await {
                        match msg {
                            ConnectionEvent::Msg(msg) => {
                                trans2_sender.send(msg);
                            }
                            ConnectionEvent::Stats(stats) => {
                                log::info!("[Transport2] rtt_ms {}", stats.rtt_ms);
                            }
                        }
                    }
                }
                _ => panic!("Unexpected event"),
            }
        }
    });

    async_std::task::sleep(Duration::from_secs(1)).await;
    log::info!("Connect to {}", node_addr_builder2.addr());

    for conn in transport1.connector().create_pending_outgoing(node_addr_builder2.addr()) {
        transport1.connector().continue_pending_outgoing(conn);
    }

    loop {
        match transport1.recv().await {
            Ok(TransportEvent::Outgoing(trans1_sender, mut trans1_receiver)) => {
                let mut msg_count = 0;
                trans1_sender.send(TransportMsg::build(0, 0, RouteRule::Direct, 0, 0, &[0; 10]));
                let mut last_send = std::time::Instant::now();
                while msg_count < 1000000 {
                    match trans1_receiver.poll().await {
                        Ok(ConnectionEvent::Msg(msg)) => {
                            msg_count += 1;
                            trans1_sender.send(msg);
                            if msg_count % 10000 == 0 {
                                log::info!("Send 10000 msg, time cost {:?} -> speed {} pps", last_send.elapsed(), 10000.0 / last_send.elapsed().as_secs_f64());
                                last_send = std::time::Instant::now();
                            }
                        }
                        Ok(ConnectionEvent::Stats(stats)) => {
                            log::info!("[Transport1] rtt_ms {}", stats.rtt_ms);
                        }
                        _ => panic!("Unexpected event"),
                    }
                }
                break;
            }
            _ => panic!("Unexpected event"),
        }
    }
    task.cancel().await;
}
