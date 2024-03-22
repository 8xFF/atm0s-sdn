#!/bin/bash

source "./servers.sh"

mkdir -p ./logs/
rm -f ./logs/*

# Loop through each server
for server in "${!servers[@]}"; do
    # Check if the server key ends with "_node_id" or "_web_addr"
    if [[ $server == *"_node_id" ]] || [[ $server == *"_public" ]] || [[ $server == *"_node_id" ]] || [[ $server == *"_seeds" ]] || [[ $server == *"_collector" ]]; then
        continue
    fi

    node_id="${servers["$server"_node_id]}"

    ssh "${servers[$server]}" "gzip -f --keep /var/log/atm0s-sdn-node.log"
    scp "${servers[$server]}:/var/log/atm0s-sdn-node.log.gz" "logs/$node_id.log.gz"
done
