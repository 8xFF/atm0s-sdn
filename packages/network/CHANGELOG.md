# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/8xFF/atm0s-sdn/compare/atm0s-sdn-network-v0.3.1...atm0s-sdn-network-v0.4.0) - 2024-04-15

### Added
- ext worker communication ([#160](https://github.com/8xFF/atm0s-sdn/pull/160))
- authorization encryption ([#153](https://github.com/8xFF/atm0s-sdn/pull/153))
- history timeout and some logs ([#150](https://github.com/8xFF/atm0s-sdn/pull/150))
- meta in virtual socket for implement enc, quic-tunnel example ([#149](https://github.com/8xFF/atm0s-sdn/pull/149))

### Fixed
- connection manager should response even when it in connected state for adapting with network lossy ([#154](https://github.com/8xFF/atm0s-sdn/pull/154))
- alias only timeout in very rare cases bug ([#151](https://github.com/8xFF/atm0s-sdn/pull/151))

### Other
- switched to use TaskSwitcher from SansIO Runtime ([#157](https://github.com/8xFF/atm0s-sdn/pull/157))
- added sync msg size test ([#156](https://github.com/8xFF/atm0s-sdn/pull/156))
- BREAKING CHANGE: Migrate sans io runtime ([#144](https://github.com/8xFF/atm0s-sdn/pull/144))

## [0.3.1](https://github.com/8xFF/atm0s-sdn/compare/atm0s-sdn-network-v0.3.0...atm0s-sdn-network-v0.3.1) - 2024-01-24

### Other
- update Cargo.toml dependencies

## [0.3.0](https://github.com/8xFF/atm0s-sdn/compare/atm0s-sdn-network-v0.2.2...atm0s-sdn-network-v0.3.0) - 2023-12-27

### Added
- secure with static key and noise protocol ([#101](https://github.com/8xFF/atm0s-sdn/pull/101))
- node multi addrs ([#98](https://github.com/8xFF/atm0s-sdn/pull/98))

## [0.2.2](https://github.com/8xFF/atm0s-sdn/compare/atm0s-sdn-network-v0.2.1...atm0s-sdn-network-v0.2.2) - 2023-12-12

### Other
- update dependencies

## [0.2.1](https://github.com/8xFF/atm0s-sdn/compare/atm0s-sdn-network-v0.2.0...atm0s-sdn-network-v0.2.1) - 2023-12-11

### Other
- move local deps out of workspace Cargo.toml ([#92](https://github.com/8xFF/atm0s-sdn/pull/92))

## [0.2.0](https://github.com/8xFF/atm0s-sdn/compare/atm0s-sdn-network-v0.1.1...atm0s-sdn-network-v0.2.0) - 2023-12-11

### Added
- rpc service and fi ([#87](https://github.com/8xFF/atm0s-sdn/pull/87))
- manual-discovery with node tags ([#84](https://github.com/8xFF/atm0s-sdn/pull/84))

### Fixed
- missing register service ([#85](https://github.com/8xFF/atm0s-sdn/pull/85))

## [0.1.1](https://github.com/8xFF/atm0s-sdn/compare/atm0s-sdn-network-v0.1.0...atm0s-sdn-network-v0.1.1) - 2023-11-17

### Other
- release ([#63](https://github.com/8xFF/atm0s-sdn/pull/63))
