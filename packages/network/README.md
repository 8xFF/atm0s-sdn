Network is split into 2 parts

- Network and routing tables
- Custom behaviours

## Network and routing tables

This part handles the network and routing tables.
The logic of routing table is not fixed, instead it is inject into with trait RoutingTable which provide bellow function

```rust
pub trait RouterTable: Send + Sync {
    fn path_to_node(&self, dest: NodeId) -> RouteAction;
    fn path_to_key(&self, key: NodeId) -> RouteAction;
    fn path_to_service(&self, service_id: u8) -> RouteAction;
    fn path_to(&self, route: &MsgRoute, service_id: u8) -> RouteAction {
        match route {
            MsgRoute::Node(dest) => self.path_to_node(*dest),
            MsgRoute::Closest(key) => self.path_to_key(*key),
            MsgRoute::Service => self.path_to_service(service_id),
        }
    }
}
```

By the way, network providing some based function to sending and handler message:

- Send to other Node by NodeId
- Send to other Node, which served by a service which is identified by service_id
- Send to local Node

Message is sending to other node by routing table and transport

## Custom behaviours

By provide a custom behaviour, we can add logic to application with modules style, each that will need to defined some parts:

- Behaviour: defined main logic (this is heart of behaviour), which handle some main event like: new incoming-outgoing connection, connection disconnected,
- Connection Handler: defined logic which handle seperate with each connection like: connection event, transport message ...

Each part also can exchanged data with other part by using Agent, which is provide in each function handler

- From behavior to each connections
- From each connection back to behavior
- From each connection to each other connection

For reference bellow is trait of behaviour

```rust
pub trait NetworkBehavior<BE, HE, MSG, Req, Res>
where
    MSG: Send + Sync,
{
    fn service_id(&self) -> u8;
    fn on_tick(&mut self, agent: &BehaviorAgent<HE, MSG>, ts_ms: u64, interal_ms: u64);
    fn check_incoming_connection(
        &mut self,
        node: NodeId,
        conn_id: ConnId,
    ) -> Result<(), ConnectionRejectReason>;
    fn check_outgoing_connection(
        &mut self,
        node: NodeId,
        conn_id: ConnId,
    ) -> Result<(), ConnectionRejectReason>;
    fn on_incoming_connection_connected(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        conn: Arc<dyn ConnectionSender<MSG>>,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE, MSG>>>;
    fn on_outgoing_connection_connected(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        conn: Arc<dyn ConnectionSender<MSG>>,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE, MSG>>>;
    fn on_incoming_connection_disconnected(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        conn: Arc<dyn ConnectionSender<MSG>>,
    );
    fn on_outgoing_connection_disconnected(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        conn: Arc<dyn ConnectionSender<MSG>>,
    );
    fn on_outgoing_connection_error(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        node_id: NodeId,
        conn_id: ConnId,
        err: &OutgoingConnectionError,
    );
    fn on_handler_event(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        node_id: NodeId,
        conn_id: ConnId,
        event: BE,
    );
    fn on_rpc(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        req: Req,
        res: Box<dyn RpcAnswer<Res>>,
    ) -> bool;
}
```

Bellow is trait of ConnectionHandler

```rust
pub trait ConnectionHandler<BE, HE, MSG>: Send + Sync {
    fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE, MSG>);
    fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, ts_ms: u64, interal_ms: u64);
    fn on_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: ConnectionEvent<MSG>);
    fn on_other_handler_event(
        &mut self,
        agent: &ConnectionAgent<BE, HE, MSG>,
        from_node: NodeId,
        from_conn: ConnId,
        event: HE,
    );
    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: HE);
    fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE, MSG>);
}
```