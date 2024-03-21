#!/bin/bash

source "./servers.sh"

# Loop through each server
for server in "${!servers[@]}"; do
    # Check if the server key ends with "_node_id" or "_web_addr"
    if [[ $server == *"_node_id" ]] || [[ $server == *"_node_id" ]] || [[ $server == *"_seeds" ]] || [[ $server == *"_collector" ]]; then
        continue
    fi

    echo "Disable and stop" "${servers[$server]}"

    ssh "${servers[$server]}" "systemctl disable atm0s-sdn-node"
    ssh "${servers[$server]}" "systemctl stop atm0s-sdn-node"
done