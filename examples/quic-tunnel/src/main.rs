use std::{
    net::{SocketAddr, SocketAddrV4},
    sync::Arc,
};

use atm0s_sdn::{NodeAddr, NodeId};
use clap::Parser;
use quinn::{AsyncUdpSocket, Connecting, ConnectionError, Endpoint, EndpointConfig, TokioRuntime};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
};
use vnet::VirtualUdpSocket;

mod sdn;
mod vnet;

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

    /// Bind addr
    #[arg(env, short, long, default_value = "0.0.0.0:8000")]
    bind_addr: SocketAddr,

    /// Tunnel to node
    #[arg(env, short, long)]
    dest_node: Option<NodeId>,

    /// Tunnel to local
    #[arg(env, short, long)]
    local_tunnel: Option<SocketAddr>,
}

#[tokio::main]
async fn main() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "info");
    }
    let args = Args::parse();
    tracing_subscriber::fmt::init();

    let (mut vnet, tx, rx) = vnet::VirtualNetwork::new(args.node_id);

    if let Some(dest_tunnel) = args.local_tunnel {
        let socket = vnet.udp_socket(10000).await;
        let runtime = Arc::new(TokioRuntime);
        let mut config = EndpointConfig::default();
        config.max_udp_payload_size(1500).expect("Should config quinn server max_size to 1500");
        let endpoint = Endpoint::new_with_abstract_socket(config, Some(quinn_plaintext::server_config()), socket, runtime).expect("Should create quinn endpoint");

        tokio::spawn(async move {
            while let Some(connecting) = endpoint.accept().await {
                log::info!("Got quic connecting event from {}", connecting.remote_address());
                tokio::spawn(async move {
                    if let Err(e) = tunnel_incoming_quic(connecting, dest_tunnel).await {
                        log::error!("Tunnel error {:?}", e);
                    }
                });
            }
        });

        tokio::spawn(async move {
            log::info!("QUIN Server started");
            while let Some(_) = vnet.recv().await {}
            log::info!("QUIN Server stopped");
        });
    } else if let Some(dest_node) = args.dest_node {
        let addr = args.bind_addr;
        let tcp_server = TcpListener::bind(addr).await.expect("Should bind to addr");
        tokio::spawn(async move {
            log::info!("TCP Server started at {addr}");
            loop {
                select! {
                    e = tcp_server.accept() => match e {
                        Ok((stream, addr)) => {
                            log::info!("Accept tcp connection from {addr}");
                            let socket = vnet.udp_socket(0).await;
                            log::info!("Create virtual udp socket at {}", socket.local_addr().expect("should have local_addr"));
                            tokio::spawn(async move {
                                if let Err(e) = tunnel_outgoing_quic(socket, stream, addr, dest_node).await {
                                    log::error!("Tunnel error {:?}", e);
                                }
                            });
                        },
                        Err(err) => {
                            log::error!("Accept error {:?}", err);
                            break;
                        }
                    },
                    e = vnet.recv() => match e {
                        Some(_) => {},
                        None => break,
                    }
                }
            }
            log::info!("TCP Server stopped");
        });
    }

    sdn::run_sdn(args.node_id, args.udp_port, args.seeds, args.workers, tx, rx).await;
    log::info!("Server shutdown");
}

#[derive(Debug)]
enum Error {
    Quic(ConnectionError),
    Tcp(std::io::Error),
}

async fn tunnel_incoming_quic(connecting: Connecting, dest_tunnel: SocketAddr) -> Result<(), Error> {
    let conn = connecting.await.map_err(Error::Quic)?;
    log::info!("Accepted quic connection from {}", conn.remote_address());
    let (mut send, mut recv) = conn.accept_bi().await.map_err(Error::Quic)?;
    log::info!("Accepted quic bi stream from {}", conn.remote_address());
    let target_stream = TcpStream::connect(dest_tunnel).await.map_err(Error::Tcp)?;
    let (mut target_recv, mut target_send) = target_stream.into_split();

    tokio::spawn(async move {
        if let Err(e) = tokio::io::copy(&mut recv, &mut target_send).await {
            log::info!("Copy error {:?}", e);
        }
    });

    if let Err(e) = tokio::io::copy(&mut target_recv, &mut send).await {
        log::info!("Copy error {:?}", e);
    }

    log::info!("Quic connection from {} closed", conn.remote_address());
    Ok(())
}

async fn tunnel_outgoing_quic(socket: VirtualUdpSocket, stream: TcpStream, remote: SocketAddr, dest_node: NodeId) -> Result<(), Error> {
    let mut config = EndpointConfig::default();
    //Note that client mtu size shoud be smaller than server's
    config.max_udp_payload_size(1400).expect("Should config quinn client max_size to 1400");
    let mut endpoint = Endpoint::new_with_abstract_socket(config, None, socket, Arc::new(TokioRuntime)).expect("Should create endpoint");
    endpoint.set_default_client_config(quinn_plaintext::client_config());

    log::info!("Connecting to node {}", dest_node);
    let connecting = endpoint
        .connect(SocketAddr::V4(SocketAddrV4::new(dest_node.into(), 10000)), "tunnel")
        .expect("Should connect to server");
    let conn = connecting.await.map_err(Error::Quic)?;
    log::info!("Connected to node {} with quic", dest_node);
    let (mut send, mut recv) = conn.open_bi().await.map_err(Error::Quic)?;
    log::info!("Open quic bi stream to node {}", dest_node);
    let (mut target_recv, mut target_send) = stream.into_split();

    tokio::spawn(async move {
        if let Err(e) = tokio::io::copy(&mut recv, &mut target_send).await {
            log::info!("Copy error {:?}", e);
        }
    });

    if let Err(e) = tokio::io::copy(&mut target_recv, &mut send).await {
        log::info!("Copy error {:?}", e);
    }
    log::info!("TCP connection from {remote} closed");
    Ok(())
}
