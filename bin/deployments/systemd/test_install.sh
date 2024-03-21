#!/bin/bash

source "./servers.sh"

apt update -y
apt install -y jq

# Loop through each server
for server in "${!servers[@]}"; do
    # Check if the server key ends with "_node_id" or "_web_addr"
    if [[ $server == *"_node_id" ]] || [[ $server == *"_public" ]] || [[ $server == *"_node_id" ]] || [[ $server == *"_seeds" ]] || [[ $server == *"_collector" ]]; then
        continue
    fi

    echo "Install and config iperf3" "${servers[$server]}"

    ssh "${servers[$server]}" "apt-get install -y iperf3 python3 python3-pip"
    ssh "${servers[$server]}" "pip3 install jc --break-system-packages"
done
