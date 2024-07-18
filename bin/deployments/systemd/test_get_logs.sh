#!/bin/bash

source "./servers.sh"

mkdir -p ./logs/
rm -f ./logs/*

# Loop through each server
for server in "${!servers[@]}"; do
    # Check if the server key ends with "_node_id" or "_web_addr"
    if [[ $server == *"_node_id" ]] || [[ $server == *"_name" ]] || [[ $server == *"_public" ]] || [[ $server == *"_ssh_port" ]] || [[ $server == *"_seeds" ]] || [[ $server == *"_collector" ]]; then
        continue
    fi

    node_id="${servers["$server"_node_id]}"
    ssh_port="${servers["$server"_ssh_port]:-22}"

    ssh -p $ssh_port "${servers[$server]}" "gzip -f --keep /var/log/atm0s-sdn-node.log"
    scp -P $ssh_port "${servers[$server]}:/var/log/atm0s-sdn-node.log.gz" "logs/$node_id.log.gz"
done
