#!/bin/bash

source "./servers.sh"

# Loop through each server
for server in "${!servers[@]}"; do
    # Check if the server key ends with "_node_id" or "_web_addr"
    if [[ $server == *"_node_id" ]] || [[ $server == *"_name" ]] || [[ $server == *"_public" ]] || [[ $server == *"_ssh_port" ]] || [[ $server == *"_seeds" ]] || [[ $server == *"_collector" ]]; then
        continue
    fi

    # Retrieve the node_id and web_addr for the current server
    node_id="${servers["$server"_node_id]}"
    ssh_port="${servers["$server"_ssh_port]:-22}"
    seeds="${servers["$server"_seeds]}"
    collector="${servers["$server"_collector]}"
    public_ip="${servers["$server"_public]}"

    ssh -p $ssh_port "${servers[$server]}" "systemctl stop atm0s-sdn-node"

    echo "Configuring for node $node_id"

    # Replace atm0s-sdn-node.env HERE_NODE_ID
    cp ./atm0s-sdn-node.sh /tmp/atm0s-sdn-node-sh
    echo "export NODE_ID=$node_id" >> /tmp/atm0s-sdn-node-sh
    if [ -n "$seeds" ]; then
        echo "export SEEDS=$seeds" >> /tmp/atm0s-sdn-node-sh
    fi
    if [ -n "$collector" ]; then
        echo "export COLLECTOR=$collector" >> /tmp/atm0s-sdn-node-sh
    fi
    if [ -n "$public_ip" ]; then
        echo "export CUSTOM_ADDRS=\"$public_ip:10000\"" >> /tmp/atm0s-sdn-node-sh
    fi

    echo "/opt/atm0s-sdn-node" >> /tmp/atm0s-sdn-node-sh

    echo "Connecting to $server"

    # Upload the file
    ssh -p $ssh_port "${servers[$server]}" "rm -f ${servers[$server]}:/opt/atm0s-sdn-node"
    ssh -p $ssh_port "${servers[$server]}" "rm -f ${servers[$server]}:/etc/systemd/system/atm0s-sdn-node.service"
    ssh -p $ssh_port "${servers[$server]}" "rm -f ${servers[$server]}:/opt/atm0s-sdn-node.sh"
    scp -P $ssh_port "../../../target/release/atm0s-sdn-standalone" "${servers[$server]}:/opt/atm0s-sdn-node"
    scp -P $ssh_port "./atm0s-sdn-node.service" "${servers[$server]}:/etc/systemd/system/atm0s-sdn-node.service"
    scp -P $ssh_port "/tmp/atm0s-sdn-node-sh" "${servers[$server]}:/opt/atm0s-sdn-node.sh"

    # Execute the command on the server
    ssh -p $ssh_port "${servers[$server]}" "systemctl daemon-reload"
    ssh -p $ssh_port "${servers[$server]}" "systemctl enable atm0s-sdn-node"
    ssh -p $ssh_port "${servers[$server]}" "systemctl start atm0s-sdn-node"
    ssh -p $ssh_port "${servers[$server]}" "systemctl status atm0s-sdn-node"
done
