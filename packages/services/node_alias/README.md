# atm0s-sdn Node Alias Service

This service implements a DNS-style mechanism where each node registers itself along with a number of aliases. These aliases are typically used on proxy or tunnel servers.

## Functionality

The mechanism used here helps avoid the need for strong synchronization data between nodes. Any node can disconnect and rejoin at any time. The process is quite straightforward, as explained below:

- Every time a node registers or unregisters an alias, it broadcasts this change to all other nodes.
- When a node receives a registration, it stores the sender as the location hint.
- When a node needs to find a location for an alias, it first checks its local storage for the location hint.

    - It pings the location hint to ensure it still holds the alias.
    - If there's no location hint, or if the ping times out or the node replies with 'NOT FOUND', the node broadcasts a 'SCAN' request to all other nodes.
    - If a node replies with 'FOUND', then the result is obtained. However, if after a timeout there's no 'FOUND' response, it means the alias was not found.

## Usecase

This mechanism proves particularly useful when dealing with a large number of aliases that rarely change. This can be observed in systems like DNS, Tunnels, and Proxies. However, this service should not be used if you have aliases that frequently change and are queried often.