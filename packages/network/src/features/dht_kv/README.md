# Simple DHT storage

atm0s-dht is used for storing and retrieving key-value pairs in a distributed hash table (DHT). It is a simple implementation of a DHT, which is heavily inspired by the Kademlia DHT. We use this dht as a temporal storage for the network, with the idea of only trusting local data, and DHT is a way to synchronize data between sources and consumers.

To do this, each node will sync its data with Set or Del commands to a node closest to its key, called RELAY. Each node that is interested in a key will send Sub or Unsub commands to a node closest to its key and will receive every change of that key.

We have some terms:

- SOURCE: node have data
- RELAY: node take care of publish event to Consumers
- CONSUMER: node interested in a key

We need to solve some main problems with DHT storage:

- Conflic
- Out of sync

## Conflic

We will have conflic when a key is updated by 2 nodes at the same time. To solve this, we will have multi-sources key, which data from a key will store independently, and will be merged at consumers.

We also have a version for each node session, and we will use a pair (node_id, live_session) as an identify for a node, and called NodeSession

| Key | Subkey | Source      | Value     |
| --- | ------ | ----------- | --------- |
| 1   | 2      | (200, 1234) | [1,2,3,4] |
| 1   | 2      | (201, 1235) | [1,2,3,3] |


## Out of sync

If there nodes don't changed overtime, then the location of each key will be remain, it case very simple.
The more complex case is when network structure changed, then we need to re-locate the key to the new RELAY location.

- SOURCE: will send Set to new RELAY
- CONSUMERs: will send Sub to new RELAY
- Old RELAY: will timeout the key and send OnDel to CONSUMERs
- New RELAY: will send SetOk to Source and SubOk to CONSUMERs, and will send OnSet to CONSUMERs

We can solve that by simple mechanism, which rely on Sub and SubOk.

- Each subscribers is locked to a RELAY session, and only accept data from that session
- Send Sub(relay_session) to RELAY, if selected RELAY is different from current locked relay_session, RELAY will send SubOk(new_session) and fires OnSet for all keys, OnSet is resend util it received OnSetAck or timeout.
- Subscribers will received SubOk(new_session) and changed locked session to the new, and starting accept event from new session, by that way, OnDel(timeout) will not be accepted.

We will have some edge cases:

- SubOk is derivered after OnSet, then we will ignore previous and wait Relay resend OnSet after SubOk
- SubOk is not derivered, then we will send Sub again
- OnDel(Timeout) is derivered before SubOk: this case is very rarely, because Timeout is larger than resend Sub alot, if it happend, the consumers will have need to the key added after we send Sub, but in the end, we still have correct state.