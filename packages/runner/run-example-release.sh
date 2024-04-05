export RUST_LOG=info
cargo build --release --features vpn --example simple_node
sudo --preserve-env=RUST_LOG ../../target/release/examples/simple_node $@
