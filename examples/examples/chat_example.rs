use bluesea_identity::{NodeAddr, NodeAddrBuilder, Protocol};
use clap::{arg, Arg, ArgMatches, Parser};
use key_value::{KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueSdkEvent};
use layers_spread_router::SharedRouter;
use layers_spread_router_sync::{LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent};
use manual_discovery::{ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent};
use network::{
    convert_enum,
    plane::{NetworkPlane, NetworkPlaneConfig},
};
use pub_sub::{PubsubSdk, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent};
use reedline_repl_rs::{clap::Command, Error, Repl};
use std::sync::Arc;
use utils::SystemTimer;

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeBehaviorEvent {
    Manual(ManualBehaviorEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncBehaviorEvent),
    KeyValue(KeyValueBehaviorEvent),
    Pubsub(PubsubServiceBehaviourEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeHandleEvent {
    Manual(ManualHandlerEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncHandlerEvent),
    KeyValue(KeyValueHandlerEvent),
    Pubsub(PubsubServiceHandlerEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeSdkEvent {
    KeyValue(KeyValueSdkEvent),
}
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Current Node ID
    #[arg(env, long)]
    node_id: u32,

    /// Neighbors
    #[arg(env, long)]
    neighbours: Vec<NodeAddr>,
}
struct Context {
    node_id: u32,
    room: Option<u32>,
    pubsub_sdk: PubsubSdk,
    router: SharedRouter,
    publisher: Option<pub_sub::Publisher>,
}

async fn join_room(args: ArgMatches, context: &mut Context) -> Result<Option<String>, Error> {
    let sdk = context.pubsub_sdk.clone();
    context.room = Some(*args.get_one::<u32>("room").unwrap());
    let node_id = context.node_id;
    if let Some(room) = context.room.clone() {
        context.publisher = Some(sdk.create_publisher(room));
        async_std::task::spawn(async move {
            println!("Room {} Joined", room);

            let consumer = sdk.create_consumer(room, Some(10));

            loop {
                if let Some(msg) = consumer.recv().await {
                    if (msg.1) == node_id {
                        continue;
                    }
                    println!("\nRoom {} Received: {}", room, String::from_utf8(msg.3.to_vec()).unwrap());
                }
            }
        });
    }
    Ok(Some(format!("Joined, {}", args.get_one::<u32>("room").unwrap())))
}

fn send_msg(args: ArgMatches, context: &mut Context) -> Result<Option<String>, Error> {
    if let Some(publisher) = &context.publisher {
        let msg = args.get_one::<String>("msg").unwrap();
        publisher.send(msg.as_bytes().to_vec().into());
    } else {
        println!("Please join a room first");
    }
    Ok(None)
}

fn print_route_table(_args: ArgMatches, context: &mut Context) -> Result<Option<String>, Error> {
    context.router.print_dump();
    Ok(None)
}

#[async_std::main]
async fn main() {
    env_logger::builder().format_timestamp_millis().init();

    let args = Args::parse();

    let node_addr_builder = Arc::new(NodeAddrBuilder::default());
    node_addr_builder.add_protocol(Protocol::P2p(args.node_id));
    let transport = transport_udp::UdpTransport::new(args.node_id, 50000 + args.node_id as u16, node_addr_builder.clone()).await;
    let node_addr = node_addr_builder.addr();
    log::info!("Listening on addr {}", node_addr);

    let timer = Arc::new(SystemTimer());
    let router = SharedRouter::new(args.node_id);

    let manual = ManualBehavior::new(ManualBehaviorConf {
        node_id: args.node_id,
        neighbours: args.neighbours.clone(),
        timer: timer.clone(),
    });

    let spreads_layer_router = LayersSpreadRouterSyncBehavior::new(router.clone());
    let key_value_sdk = key_value::KeyValueSdk::new();
    let key_value = key_value::KeyValueBehavior::new(args.node_id, 10000, Some(Box::new(key_value_sdk.clone())));
    let (pubsub_behavior, pubsub_sdk) = pub_sub::PubsubServiceBehaviour::new(args.node_id, timer.clone());

    let plan_cfg = NetworkPlaneConfig {
        router: Arc::new(router.clone()),
        node_id: args.node_id,
        tick_ms: 1000,
        behaviors: vec![Box::new(manual), Box::new(spreads_layer_router), Box::new(key_value), Box::new(pubsub_behavior)],
        transport: Box::new(transport),
        timer,
    };

    let mut plane = NetworkPlane::<NodeBehaviorEvent, NodeHandleEvent, NodeSdkEvent>::new(plan_cfg);

    let _ = async_std::task::spawn(async move {
        plane.started();
        while let Ok(_) = plane.recv().await {}
        plane.stopped();
    });

    let context = Context {
        room: None,
        pubsub_sdk: pubsub_sdk.clone(),
        node_id: args.node_id,
        router: router.clone(),
        publisher: None,
    };

    let mut repl = Repl::new(context)
        .with_name("Sample Chat")
        .with_command_async(
            Command::new("join").arg(Arg::new("room").value_parser(clap::value_parser!(u32)).required(true)).about("Join a room"),
            |args, context| Box::pin(join_room(args, context)),
        )
        .with_command(Command::new("send").arg(Arg::new("msg").required(true)).about("Send message"), send_msg)
        .with_command(Command::new("router").about("Print router table"), print_route_table);
    let _ = repl.run_async().await;
}
