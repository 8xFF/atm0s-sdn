#!/bin/bash

source "./servers.sh"

mkdir -p results
rm -f results/*

echo "source; dest; public; vpn" > results/stats.csv

# Loop through each server
for server in "${!servers[@]}"; do
    # Check if the server key ends with "_node_id" or "_web_addr"
    if [[ $server == *"_node_id" ]] || [[ $server == *"_name" ]] || [[ $server == *"_public" ]] || [[ $server == *"_ssh_port" ]] || [[ $server == *"_seeds" ]] || [[ $server == *"_collector" ]]; then
        continue
    fi

    node_id="${servers["$server"_node_id]}"
    name="${servers["$server"_name]}"
    ssh_port="${servers["$server"_ssh_port]:-22}"
    public_ip="${servers["$server"_public]}"
    vpn_ip="10.33.33.$node_id"

    for target in "${!servers[@]}"; do
        if [[ $target == *"_node_id" ]] || [[ $target == *"_name" ]] || [[ $target == *"_public" ]] || [[ $target == *"_ssh_port" ]] || [[ $target == *"_node_id" ]] || [[ $target == *"_seeds" ]] || [[ $target == *"_collector" ]]; then
            continue
        fi

        target_name="${servers["$target"_name]}"
        target_node_id="${servers["$target"_node_id]}"
        target_public_ip="${servers["$target"_public]}"
        target_vpn_ip="10.33.33.$target_node_id"

        if [[ "$node_id" == "$target_node_id" ]]; then
            continue
        fi

        echo "Running test from $node_id $name to $target_node_id $target_name, $public_ip, $target_public_ip"

        ssh -p $ssh_port "${servers[$server]}" "ping -c 1 $target_public_ip | jc --ping > /tmp/$node_id-$target_node_id-ping-public.json"
        ssh -p $ssh_port "${servers[$server]}" "ping -c 1 $target_vpn_ip | jc --ping > /tmp/$node_id-$target_node_id-ping-vpn.json"
        scp -P $ssh_port "${servers[$server]}:/tmp/$node_id-$target_node_id-ping-public.json" "results/$node_id-$target_node_id-ping-public.json"
        scp -P $ssh_port "${servers[$server]}:/tmp/$node_id-$target_node_id-ping-vpn.json" "results/$node_id-$target_node_id-ping-vpn.json"

        rtt_public=$(cat results/$node_id-$target_node_id-ping-public.json | jq ".round_trip_ms_avg")
        rtt_vpn=$(cat results/$node_id-$target_node_id-ping-vpn.json | jq ".round_trip_ms_avg")
        echo "$node_id; $target_node_id; $name; $target_name; $rtt_public; $rtt_vpn"
        echo "$node_id; $target_node_id; $name; $target_name; $rtt_public; $rtt_vpn" >> results/stats.csv
    done
done
