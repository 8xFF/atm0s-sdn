use std::time::Duration;

use async_std::net::UdpSocket;
use utils::error_handle::ErrorUtils;

#[async_std::main]
async fn main() {
    env_logger::builder().format_timestamp_millis().filter_level(log::LevelFilter::Info).init();
    let udp_server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let udp_client = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    async_std::task::sleep(Duration::from_secs(1)).await;
    log::info!("Connect to {}", udp_server.local_addr().unwrap());
    udp_client.connect(udp_server.local_addr().unwrap()).await.unwrap();

    let task = async_std::task::spawn(async move {
        let mut buf = [0; 1500];
        loop {
            match udp_server.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    udp_server.send_to(&buf[0..len], addr).await.print_error("Should send");
                }
                _ => panic!("Unexpected event"),
            }
        }
    });
    
    let mut msg_count = 0;
    udp_client.send(&[0; 10]).await.print_error("Should send");
    let mut last_send = std::time::Instant::now();
    let mut buf = [0; 1500];
    while msg_count < 1000000 {
        match udp_client.recv(&mut buf).await {
            Ok(len) => {
                msg_count += 1;
                udp_client.send(&buf[0..len]).await.print_error("Should send");
                if msg_count % 10000 == 0 {
                    log::info!("Send 10000 msg, time cost {:?} -> speed {} pps", last_send.elapsed(), 10000.0 / last_send.elapsed().as_secs_f64());
                    last_send = std::time::Instant::now();
                }
            }
            _ => panic!("Unexpected event"),
        }
    }           
                
    task.cancel().await;
}