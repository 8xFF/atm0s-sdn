# Pubsub system

This package provides a pubsub system that allows you to publish messages to a topic and subscribe to messages on a topic.
For simplicity, we split it to 2 parts:

- Pubsub core: which is take care relaying data between concrete pubsub source to consumers. In this part, we need to determine where is the source of the channel.
- Auto detect source: combine above Pubsub core with KeyValue system, we will store the sources of the channel in the DHT, and we will use the DHT to detect the source of the channel.

## Pubsub Protocol

We have 5 types of message:

- Sub (channel, source, uuid)
- SubOk (channel, source, uuid)
- Unsub (channel, source, uuid)
- UnsubOk (channel, source, uuid, data)
- Data(channel, source, uuid, data)

We a node interested in a topic in a source node, it will find the way to the source then send Sub (channel, source, uuid) to next node by using router table. Uuid is the session of node, which can be used to determine the node is still alive or not.

If the node don't interest in the topic anymore, it will send Unsub (channel, source, uuid) to the source node.

Each node when receive Sub, it will send SubOk to the source node, and start to relay data from source to the node. If the node receive Unsub, it will send UnsubOk to the source node, and stop relaying data.

The uuid also to be used to validate SubOk and UnsubOk. In the future, we can have more complex logic to validate the message, like signature or encryption.

## Sticky or Dynamic path

Atm0s routing table can providing two way to route the message:

- Sticky: the route path will be fixed until pubsub channel is closed. When any node in route path is down, the previous node will try to find the new path to the source node.
- Dynamic: the route path will be updated periodically, and the route path will be changed if the network structure is changed. This is useful when the network is changed frequently.

We can have combine of both, which the route path will be sticky in a period of time, and will be updated if the network structure is changed.

Currently implement will keep sticky in 5 minutes, and will be updated if the network structure is changed.