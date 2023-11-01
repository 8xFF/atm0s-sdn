[![Rust](https://github.com/8xFF/decentralized-sdn/actions/workflows/rust.yml/badge.svg)](https://github.com/8xFF/decentralized-sdn/actions/workflows/rust.yml)
[![codecov](https://codecov.io/gh/8xFF/decentralized-sdn/graph/badge.svg?token=P8W3LUA0EV)](https://codecov.io/gh/8xFF/decentralized-sdn)

# Bluesea SDN v4: Code name: Pacific Ocean

4.0 version be created with main goals:

- Separated satellite with network operation, only take role in create node_id
- Node neighbours is detect with Kademlia DHT k-bucket table, and selected based on RTT or something else ..


## Project structs

Project is splited to Core and runner, each core is independed with any other parts

Core:
- Transport: Working with data transfer, which take role in sending, receiving data, deal with Packet loss and measure quality of Connection
- RPC: Working with request response
- Kademlia: implement kademlia k-bucket logic
- Router: Bluesea routing table for archiving ultra-low-latency goal
- Service: Bluesea service based and helpers

Service:
- KeyValue
- Pubsub
- Storage
....

Runner: this is combine all parts to a working node
