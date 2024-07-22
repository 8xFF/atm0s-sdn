# If provided $3, it will be seeds
if [ -n "$4" ]; then
    # $4 is defined
    cargo run -- --collector --local-tags vpn --connect-tags vpn --node-id $1 --udp-port $2 --web-addr $3 --seeds $4
else
    # $4 is not defined
    cargo run -- --collector --local-tags vpn --connect-tags vpn --node-id $1 --udp-port $2 --web-addr $3
fi
