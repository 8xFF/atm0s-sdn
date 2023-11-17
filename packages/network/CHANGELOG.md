# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/8xFF/atm0s-sdn/releases/tag/atm0s-sdn-network-v0.1.0) - 2023-11-17

### Fixed
- fixing warn
- fixing some clippy
- fixing some warn and fixing some debug message
- fixing test errors
- fixing build
- fixing bug on wrong behaviour on_outgoing_connection. added test bootstrap and auto_refresh

### Other
- auto release with release-plz ([#62](https://github.com/8xFF/atm0s-sdn/pull/62))
- Rename package to atm0s-sdn ([#61](https://github.com/8xFF/atm0s-sdn/pull/61))
- Update documents for core and network package ([#51](https://github.com/8xFF/atm0s-sdn/pull/51))
- refactor to use cross-service sdk in pub-sub ([#43](https://github.com/8xFF/atm0s-sdn/pull/43))
- Update support for SDK Internal events, And added some unit tests for Network internal ([#41](https://github.com/8xFF/atm0s-sdn/pull/41))
- Update Rust crate bytes to 1.5.0 ([#29](https://github.com/8xFF/atm0s-sdn/pull/29))
- update testing for bus impl ([#37](https://github.com/8xFF/atm0s-sdn/pull/37))
- migrate network package ([#11](https://github.com/8xFF/atm0s-sdn/pull/11))
- refactor some log with more info
- Pubsub service ([#4](https://github.com/8xFF/atm0s-sdn/pull/4))
- Key value service ([#3](https://github.com/8xFF/atm0s-sdn/pull/3))
- fmt
- first working vpn
- implement tun-tap-service
- fmt
- remove warn
- split single conn from plane logic
- continue fixing warn
- refactor network: inprogress add route inside
- change format for longer max line width for better reading
- refactoring network
- refactor conn_id
- added origin router from sdn-v3
- changed from peer to node keyword
- added manual-node example
- added tcp ping pong for check rtt
- added transport tcp and fixing test for multiaddr
- added multiaddr custom ver, added manual discovery by specific neighbor address
- added transport rpc interface
- count networks in vnet
- added connection check in behavior
- refactor log select in plane
- fmt
- remove kanal because of unstable
- discovery with kademlia without test
- added vnet
- switched to using kanal mspc for better performance
- switched to using MultiAddr from libp2p
- added new kademlia implement
- fmt
- added checking behavior close, handle to handle msg
- added close handle test
- finished part of network
- handle Msg instead of raw bytes in behaviours
- switched to convert-enum instead of manual
- early state of network behaviour
- mock discovery logic
- mock logic
- added Network Agent
- added network based
