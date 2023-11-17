# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/8xFF/atm0s-sdn/releases/tag/atm0s-sdn-transport-tcp-v0.1.0) - 2023-11-17

### Fixed
- fixing some clippy
- fixing some warn and fixing some debug message
- fixing test errors
- fixing build

### Other
- auto release with release-plz ([#62](https://github.com/8xFF/atm0s-sdn/pull/62))
- Rename package to atm0s-sdn ([#61](https://github.com/8xFF/atm0s-sdn/pull/61))
- Update Rust crate async-bincode to 0.7.2 ([#44](https://github.com/8xFF/atm0s-sdn/pull/44))
- migrate network package ([#11](https://github.com/8xFF/atm0s-sdn/pull/11))
- Key value service ([#3](https://github.com/8xFF/atm0s-sdn/pull/3))
- implement tun-tap-service
- continue fixing warn
- refactor network: inprogress add route inside
- change format for longer max line width for better reading
- added fast_path_route and example
- refactor conn_id
- changed from peer to node keyword
- added manual-node example
- added tcp ping pong for check rtt
- added transport tcp and fixing test for multiaddr
- switched to using MultiAddr from libp2p
