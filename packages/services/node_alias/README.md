# atm0s-sdn-node-alias service

This service implement DNS style mechanism, each node will it self register will number of aliases, which usually used on proxy or tunnel server.

## How it works

We using mechanism for avoiding strong sync data between nodes, any nodes can be disconnect and re-join anytime. The mechanism is very simple as described bellow:

- Each time node REGISTER or UNREGISTER with an alias, it broadcast to all nodes about the changed.
- When a node received REGISTER it store SENDER as LOCATION_HINT
- When a node want to finding a location for a alias, it will search in local for getting LOCALTION_HINT

    - It ping LOCATION_HINT first for ensuring it still has alias
    - If don't have LOCATION_HINT or, above ping timeout or node reply with NOT_FOUND, it broadcast SCAN to all other nodes
    - If a node reply with FOUND, then we got result, if after timeout we dont have FOUND, it mean not found

## Conclusion

Above mechanism very helpful when we have large of aliases and infrequency changed, this can be found in some system like DNS, Tunnel, Proxy. Please don't use this service if you have alisases which changed in very high frequency and high query frequency.