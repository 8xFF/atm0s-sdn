use atm0s_sdn::SharedRouter;
use atm0s_sdn::SystemTimer;
use atm0s_sdn::{convert_enum, NetworkPlane, NetworkPlaneConfig};
use atm0s_sdn::{KeyValueBehavior, KeyValueSdk, NodeAddr, NodeAddrBuilder, PubsubServiceBehaviour, UdpTransport};
use atm0s_sdn::{KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueSdkEvent};
use atm0s_sdn::{LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent};
use atm0s_sdn::{ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent};
use atm0s_sdn::{PubsubSdk, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent};
use clap::{arg, Arg, ArgMatches, Parser};
use reedline_repl_rs::{clap::Command, Error, Repl};
use std::sync::Arc;

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
    seeds: Vec<NodeAddr>,
}
struct Context {
    node_id: u32,
    room: Option<u32>,
    pubsub_sdk: PubsubSdk,
    router: SharedRouter,
    publisher: Option<atm0s_sdn::Publisher>,
    consumer: Option<atm0s_sdn::Consumer>,
    task: Option<async_std::task::JoinHandle<()>>,
}

async fn join_room(args: ArgMatches, context: &mut Context) -> Result<Option<String>, Error> {
    if context.room.is_some() {
        println!("Please leave current room first");
        return Ok(None);
    }
    // Here we use the pubsub service SDK to create a publisher and a consumer.
    // The publisher and consumer are bound to a room, which is identified by a u32 number.
    // Either the publisher or the consumer can be created first.
    let sdk = context.pubsub_sdk.clone();
    context.room = Some(*args.get_one::<u32>("room").unwrap());
    let node_id = context.node_id;
    if let Some(room) = context.room.clone() {
        let consumer = sdk.create_consumer(room, Some(10));
        context.publisher = Some(sdk.create_publisher(room));
        context.consumer = Some(consumer.clone());
        context.task = Some(async_std::task::spawn(async move {
            loop {
                if let Some(msg) = consumer.recv().await {
                    if (msg.1) == node_id {
                        continue;
                    }
                    // TODO: Fix formatting error
                    println!("Node {} to room {}: {}\n", msg.1, room, String::from_utf8(msg.3.to_vec()).unwrap());
                }
            }
        }));
    }
    Ok(Some(format!("Joined, {}", args.get_one::<u32>("room").unwrap())))
}

async fn leave_room(_args: ArgMatches, context: &mut Context) -> Result<Option<String>, Error> {
    if context.room.is_some() {
        context.room = None;
        context.publisher = None;
        context.consumer = None;
        context.task.take().unwrap().cancel().await;
    } else {
        println!("Please join a room first");
    }
    Ok(Some(format!("Left room {}", context.room.unwrap())))
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

    // Build a node address, which is used to identify a node in the network.
    // The node address is a multiaddr, which is a self-describing network address.
    // The multiaddr is composed of multiple protocols, each of which is identified by a code.
    // example in our case: /p2p/0/ip4/127.0.0.1/udp/50000
    // You can find more information about multiaddr here: https://multiformats.io/multiaddr/
    let node_addr_builder = Arc::new(NodeAddrBuilder::default());
    node_addr_builder.set_node_id(args.node_id);

    // Create a transport layer, which is used to send and receive messages.
    // In this example, we use the UDP transport layer.
    // There are also other transport layers, such as TCP and VNET, others are still in progress.
    // The port number is 50000 + node_id.
    let transport = UdpTransport::new(args.node_id, 50000 + args.node_id as u16, node_addr_builder.clone()).await;
    let node_addr = node_addr_builder.addr();
    println!("Listening on addr {}", node_addr);

    // Create a timer, which is used to trigger events periodically.
    let timer = Arc::new(SystemTimer());

    // The router is used to route messages to the correct node.
    // It keeps a routing table, which is updated when a node joins or leaves the network.
    let router = SharedRouter::new(args.node_id);

    // Now we need to create a network plane, which is used to manage the network.
    // The network plane is composed of multiple services with behaviors, each of which is responsible for a specific task.
    // In this example, we use the manual behavior, which is used to manually add neighbors.
    let manual = ManualBehavior::new(ManualBehaviorConf {
        node_id: args.node_id,
        node_addr,
        seeds: args.seeds.clone(),
        local_tags: vec![],
        connect_tags: vec![],
    });

    // The layers spread router behavior is used to route messages to the correct node using the router created earlier.
    let spreads_layer_router = LayersSpreadRouterSyncBehavior::new(router.clone());

    // The key value behavior is used to store and retrieve key-value pairs.
    // Here it is used by the pubsub service to store and transfer the messages sent by other nodes.
    // Some service can have SDKs, which are used to interact with the service.
    // When making an application, you will mostly use the SDKs.
    let key_value_sdk = KeyValueSdk::new();
    let key_value = KeyValueBehavior::new(args.node_id, 10000, Some(Box::new(key_value_sdk.clone())));
    // The pubsub service is used to send and receive messages.
    let (pubsub_behavior, pubsub_sdk) = PubsubServiceBehaviour::new(args.node_id, timer.clone());

    // The network plane configuration.
    let plan_cfg = NetworkPlaneConfig {
        router: Arc::new(router.clone()),
        node_id: args.node_id,
        // The tick_ms is the interval of the on_tick event, which is triggered periodically.
        tick_ms: 1000,
        behaviors: vec![Box::new(manual), Box::new(spreads_layer_router), Box::new(key_value), Box::new(pubsub_behavior)],
        transport: Box::new(transport),
        timer,
    };

    // Create a network plane.
    // There are three types of events in the network plane: behavior event, handler event, and SDK event.
    // The behavior event is triggered by the behavior, which is used to interact with the handler.
    // The handler event is triggered by the handler, which is used to interact with the behavior.
    // The SDK event is triggered by the SDK, which is used to interact with other services.
    let mut plane = NetworkPlane::<NodeBehaviorEvent, NodeHandleEvent, NodeSdkEvent>::new(plan_cfg);

    // Start the network plane.
    let _ = async_std::task::spawn(async move {
        plane.started();
        while let Ok(_) = plane.recv().await {}
        plane.stopped();
    });

    // ======================== REPL Section =============================
    // Build a REPL, which is used to interact with the node.

    let context = Context {
        room: None,
        pubsub_sdk: pubsub_sdk.clone(),
        node_id: args.node_id,
        router: router.clone(),
        publisher: None,
        consumer: None,
        task: None,
    };

    // Create a REPL, which is used to interact with the node.
    // INFO: You can use Ctrl-D to exit the REPL.
    let mut repl = Repl::new(context)
        .with_name("Sample Chat")
        .with_command_async(
            Command::new("join").arg(Arg::new("room").value_parser(clap::value_parser!(u32)).required(true)).about("Join a room"),
            |args, context| Box::pin(join_room(args, context)),
        )
        .with_command(Command::new("send").arg(Arg::new("msg").required(true)).about("Send message"), send_msg)
        .with_command(Command::new("router").about("Print router table"), print_route_table)
        .with_command_async(Command::new("leave").about("Leave a room"), |args, context| Box::pin(leave_room(args, context)));
    let _ = repl.run_async().await;
}
