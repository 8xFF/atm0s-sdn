#!/bin/bash

source "./servers.sh"

# Loop through each server
for server in "${!servers[@]}"; do
    # Check if the server key ends with "_node_id" or "_web_addr"
    if [[ $server == *"_node_id" ]] || [[ $server == *"_public" ]] || [[ $server == *"_ssh_port" ]] || [[ $server == *"_node_id" ]] || [[ $server == *"_seeds" ]] || [[ $server == *"_collector" ]]; then
        continue
    fi

    ssh_port="${servers["$server"_ssh_port]:-22}"

    echo "Disable and stop" "${servers[$server]}"

    ssh -p $ssh_port "${servers[$server]}" "systemctl disable atm0s-sdn-node"
    ssh -p $ssh_port "${servers[$server]}" "systemctl stop atm0s-sdn-node"
done