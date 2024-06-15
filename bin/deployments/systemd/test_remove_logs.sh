#!/bin/bash

source "./servers.sh"

# Loop through each server
for server in "${!servers[@]}"; do
    # Check if the server key ends with "_node_id" or "_web_addr"
    if [[ $server == *"_node_id" ]] || [[ $server == *"_name" ]] || [[ $server == *"_public" ]] || [[ $server == *"_ssh_port" ]] || [[ $server == *"_seeds" ]] || [[ $server == *"_collector" ]]; then
        continue
    fi

    node_id="${servers["$server"_node_id]}"
    ssh_port="${servers["$server"_ssh_port]:-22}"

    echo "Remove log of node $node_id"

    ssh -p $ssh_port "${servers[$server]}" "rm -f /var/log/atm0s-sdn-node.log*"
done
