#!/bin/bash

source "./servers.sh"

# Loop through each server
for server in "${!servers[@]}"; do
    # Check if the server key ends with "_node_id" or "_web_addr"
    if [[ $server == *"_node_id" ]] || [[ $server == *"_node_id" ]] || [[ $server == *"_seeds" ]] || [[ $server == *"_collector" ]]; then
        continue
    fi

    ssh "${servers[$server]}" "systemctl stop atm0s-sdn-node"
    
    # Retrieve the node_id and web_addr for the current server
    node_id="${servers["$server"_node_id]}"
    seeds="${servers["$server"_seeds]}"
    collector="${servers["$server"_collector]}"

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

    echo "/opt/atm0s-sdn-node" >> /tmp/atm0s-sdn-node-sh
    
    echo "Connecting to $server"
    
    # Upload the file
    ssh "${servers[$server]}" "rm -f ${servers[$server]}:/opt/atm0s-sdn-node"
    ssh "${servers[$server]}" "rm -f ${servers[$server]}:/etc/systemd/system/atm0s-sdn-node.service"
    ssh "${servers[$server]}" "rm -f ${servers[$server]}:/opt/atm0s-sdn-node.sh"
    scp "../../../target/release/bin" "${servers[$server]}:/opt/atm0s-sdn-node"
    scp "./atm0s-sdn-node.service" "${servers[$server]}:/etc/systemd/system/atm0s-sdn-node.service"
    scp "/tmp/atm0s-sdn-node-sh" "${servers[$server]}:/opt/atm0s-sdn-node.sh"
    
    # Execute the command on the server
    ssh "${servers[$server]}" "systemctl daemon-reload"
    ssh "${servers[$server]}" "systemctl enable atm0s-sdn-node"
    ssh "${servers[$server]}" "systemctl start atm0s-sdn-node"
    ssh "${servers[$server]}" "systemctl status atm0s-sdn-node"
done