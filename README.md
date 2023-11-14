<p align="center">
 <a href="https://github.com/8xFF/decentralized-sdn/actions">
  <img src="https://github.com/8xFF/decentralized-sdn/actions/workflows/rust.yml/badge.svg?branch=master">
 </a>
 <a href="https://codecov.io/gh/webrtc-rs/webrtc">
  <img src="https://codecov.io/gh/8xff/decentralized-sdn/branch/master/graph/badge.svg">
 </a>
 <a href="https://deps.rs/repo/github/8xff/decentralized-sdn">
  <img src="https://deps.rs/repo/github/8xff/decentralized-sdn/status.svg">
 </a>
<!--  <a href="https://crates.io/crates/8xff-sdn">
  <img src="https://img.shields.io/crates/v/8xff-sdn.svg">
 </a> -->
<!--  <a href="https://docs.rs/8xff-sdn">
  <img src="https://docs.rs/8xff-sdn/badge.svg">
 </a> -->
 <a href="https://github.com/8xFF/decentralized-sdn/blob/master/LICENSE">
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="License: MIT">
 </a>
 <a href="https://discord.gg/tJ6dxBRk">
  <img src="https://img.shields.io/discord/1173844241542287482?logo=discord" alt="Discord">
 </a>
</p>

# Global-scaled Ultra-low latency Decentralized SDN

A SAN I/O driven, open-source decentralized network infrastructure that can deliver high-quality data with minimal latency and efficient cost, similar to what Cloudflare achieves for their network.

## Features

- Blazingly fast, powered by Rust.
- High availability by being fully distributed, with no central controller.
- Multi-zone support, high scalability.
- Definable Metric based Adaptive routing: cost, latency, .etc...
- Fixed size routing table.
- Designed with large scale built-in PubSub service.
- Automatic Network orchestration and discovery (also can be manual).
- High extendibility by using Network Service.
- Built-in features: PubSub, KeyValue, VPN.
- Cross platform: Linux, MacOs, Windows.

## Architecture

Each node in the network is embeded with Geo-Location data inside its ID. A Node ID consists of multiple layers, and every node will have multiple routing tables, each is correspond to a layer.

- Layer1: Geo1 Table (Zone level)
- Layer2: Geo2 Table (Country level)
- Layer3: Inner Geo Group Table (City level)
- Layer4: Inner Group Index Table (DC level)

*TODO: Graphics instead of bulletlist*
TODO: an ARCHITECTURE.md with general information about: Project, System structure, Design philosophy, ...
## Getting started

```bash
cargo add 8xff-sdn
```

### Create a group chat application (Optional)
Visit [Tutorial]()
TODO: Create chat example

### Demo group chat application

#### Prerequisites:
To run this demo, you can:

- Build from source code

```bash
cargo build --package chat-example
```

- Download prebuild

```bash
wget https://.....
``` 

- Follow the above [Create a chat application]()

#### Running manual discovery multi nodes in single device

Start node1:

```bash
RUST_LOG=info chat-example --node-id 1 --node-port 5001
```

Start node2:

```bash
RUST_LOG=info chat-example --node-id 2 --neighbour-addr node+p2p://localhost:5001
```

In node1

```shell
> route
TODO route table here
> join room1
```

In node1

```shell
> route
TODO route table here
> join room1
```

In node2

```shell
> join room1
> send hello
```

Now, in node1 will received message from node2

```shell
> message from node(1): hello
> send hi!
```

Now, in node will received message from node1

Available commands:

- `route`: Print route table
- `join`: Join a room
- `send`: Send a message to room
- `leave`: Leave joined room

#### Running manual discovery multi nodes in multi devices

It also can start chat-example in multi nodes and connect over LAN or Internet

Start node1:

```bash
RUST_LOG=info chat-example --node-id 1 --node-port 5001
```

Start node2:

```bash
RUST_LOG=info chat-example --node-id 2 --neighbour-addr node+p2p://IP_HERE:5001
```


## Showcases

- Media Server: [Repo](https://github.com/8xFF/decentralized-media-server)
- VPN: [Repo](https://github.com/8xFF/decentralized-sdn/tree/master/packages/services/tun_tap)
- MiniRedis: [Repo](https://github.com/8xFF/decentralized-sdn/tree/master/packages/apps/redis)

## Contributing
The project is continuously being improved and updated. We are always looking for ways to make it better, whether that's through optimizing performance, adding new features, or fixing bugs. We welcome contributions from the community and are always looking for new ideas and suggestions.

For more information, you can join our [Discord channel](https://discord.gg/tJ6dxBRk)


## Roadmap

First version will be released together with [Media Server](https://github.com/8xFF/decentralized-media-server) at end of 2023.

Details on our roadmap can be seen [TBA]().

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

We would like to thank all the contributors who have helped in making this project successful.<p align="center">
 <a href="https://github.com/8xFF/decentralized-sdn/actions">
  <img src="https://github.com/8xFF/decentralized-sdn/actions/workflows/rust.yml/badge.svg?branch=master">
 </a>
 <a href="https://codecov.io/gh/webrtc-rs/webrtc">
  <img src="https://codecov.io/gh/8xff/decentralized-sdn/branch/master/graph/badge.svg">
 </a>
 <a href="https://deps.rs/repo/github/8xff/decentralized-sdn">
  <img src="https://deps.rs/repo/github/8xff/decentralized-sdn/status.svg">
 </a>
<!--  <a href="https://crates.io/crates/8xff-sdn">
  <img src="https://img.shields.io/crates/v/8xff-sdn.svg">
 </a> -->
<!--  <a href="https://docs.rs/8xff-sdn">
  <img src="https://docs.rs/8xff-sdn/badge.svg">
 </a> -->
 <a href="https://github.com/8xFF/decentralized-sdn/blob/master/LICENSE">
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="License: MIT">
 </a>
 <a href="https://discord.gg/tJ6dxBRk">
  <img src="https://img.shields.io/discord/1173844241542287482?logo=discord" alt="Discord">
 </a>
</p>

# Global-scaled Ultra-low latency Decentralized SDN

A SAN I/O driven, open-source decentralized network infrastructure that can deliver high-quality data with minimal latency and efficient cost, similar to what Cloudflare achieves for their network.

## Features

- Blazingly fast, powered by Rust.
- High availability by being fully distributed, with no central controller.
- Multi-zone support, high scalability.
- Definable Metric based Adaptive routing: cost, latency, .etc...
- Fixed size routing table.
- Designed with large scale built-in PubSub service.
- Automatic Network orchestration and discovery (also can be manual).
- High extendibility by using Network Service.
- Built-in features: PubSub, KeyValue, VPN.
- Cross platform: Linux, MacOs, Windows.

## Architecture

Each node in the network is embeded with Geo-Location data inside its ID. A Node ID consists of multiple layers, and every node will have multiple routing tables, each is correspond to a layer.

- Layer1: Geo1 Table (Zone level)
- Layer2: Geo2 Table (Country level)
- Layer3: Inner Geo Group Table (City level)
- Layer4: Inner Group Index Table (DC level)

*TODO: Graphics instead of bulletlist*
TODO: an ARCHITECTURE.md with general information about: Project, System structure, Design philosophy, ...
## Getting started

```bash
cargo add 8xff-sdn
```

### Create a group chat application (Optional)
Visit [Tutorial]()
TODO: Create chat example

### Demo group chat application

#### Prerequisites:
To run this demo, you can:

- Build from source code

```bash
cargo build --package chat-example
```

- Download prebuild

```bash
wget https://.....
``` 

- Follow the above [Create a chat application]()

#### Running manual discovery multi nodes in single device

Start node1:

```bash
RUST_LOG=info chat-example --node-id 1 --node-port 5001
```

Start node2:

```bash
RUST_LOG=info chat-example --node-id 2 --neighbour-addr node+p2p://localhost:5001
```

In node1

```shell
> route
TODO route table here
> join room1
```

In node1

```shell
> route
TODO route table here
> join room1
```

In node2

```shell
> join room1
> send hello
```

Now, in node1 will received message from node2

```shell
> message from node(1): hello
> send hi!
```

Now, in node will received message from node1

Available commands:

- `route`: Print route table
- `join`: Join a room
- `send`: Send a message to room
- `leave`: Leave joined room

#### Running manual discovery multi nodes in multi devices

It also can start chat-example in multi nodes and connect over LAN or Internet

Start node1:

```bash
RUST_LOG=info chat-example --node-id 1 --node-port 5001
```

Start node2:

```bash
RUST_LOG=info chat-example --node-id 2 --neighbour-addr node+p2p://IP_HERE:5001
```


## Showcases

- Media Server: [Repo](https://github.com/8xFF/decentralized-media-server)
- VPN: [Repo](https://github.com/8xFF/decentralized-sdn/tree/master/packages/services/tun_tap)
- MiniRedis: [Repo](https://github.com/8xFF/decentralized-sdn/tree/master/packages/apps/redis)

## Contributing
The project is continuously being improved and updated. We are always looking for ways to make it better, whether that's through optimizing performance, adding new features, or fixing bugs. We welcome contributions from the community and are always looking for new ideas and suggestions.

For more information, you can join our [Discord channel](https://discord.gg/tJ6dxBRk)


## Roadmap

First version will be released together with [Media Server](https://github.com/8xFF/decentralized-media-server) at end of 2023.

Details on our roadmap can be seen [TBA]().

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

We would like to thank all the contributors who have helped in making this project successful.
