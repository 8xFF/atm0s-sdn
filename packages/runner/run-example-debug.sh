export RUST_LOG=info
cargo build --features vpn --example simple_node
sudo --preserve-env=RUST_LOG ../../target/debug/examples/simple_node $@
