use std::{
    os::fd::{AsRawFd, FromRawFd},
    time::Duration,
};

use async_std::net::UdpSocket;
use utils::error_handle::ErrorUtils;

#[async_std::main]
async fn main() {
    env_logger::builder().format_timestamp_millis().filter_level(log::LevelFilter::Info).init();
    let udp_server = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let udp_server_async = unsafe { UdpSocket::from_raw_fd(udp_server.as_raw_fd()) };
    let udp_client = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let udp_client_async = unsafe { UdpSocket::from_raw_fd(udp_client.as_raw_fd()) };

    async_std::task::sleep(Duration::from_secs(1)).await;
    log::info!("Connect to {}", udp_server.local_addr().unwrap());
    udp_client.connect(udp_server.local_addr().unwrap()).unwrap();

    let task = async_std::task::spawn(async move {
        let mut buf = [0; 1500];
        loop {
            match udp_server_async.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    udp_server.send_to(&buf[0..len], addr).print_error("Should send");
                }
                _ => panic!("Unexpected event"),
            }
        }
    });

    let mut msg_count = 0;
    udp_client.send(&[0; 10]).print_error("Should send");
    let mut last_send = std::time::Instant::now();
    let mut buf = [0; 1500];
    while msg_count < 1000000 {
        match udp_client_async.recv(&mut buf).await {
            Ok(len) => {
                msg_count += 1;
                udp_client.send(&buf[0..len]).print_error("Should send");
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
