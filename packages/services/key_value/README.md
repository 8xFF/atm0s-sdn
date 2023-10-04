# KeyValue service

This service implement key-value storage, which has set/get/del, sub/unsub

For working in unstable network, we ensure data is correct event if lost some previous packets. For that reason, we dont use any delta data technical, instead we sending full data each time we need to changed

- Set: Key, Value, Version
- Get: Key
- Del: Key, Version

With Set, Del only correct version will be used. In details:

- Set: only newer version is apply
- Del: only equa or newer version is apply

For ensure data not losing when network changed, we implement simple replication algorithem, when each key is set to multiples node which generated by:

- OriginKey
- Rep1Key = OriginKey XOR Rep1Factor
- Rep2Key = OriginKey XOR Rep2Factor

Each time Set, Get or Del will be sent to all of origin key and replication keys

Current version only sending OriginKey

## Set

We sending set command util received ack for that set version or newer version

## Get

## Del

We sending del command util received ack for that del version or setted with new data

## Sub

We sending sub command util received ack or switched to unsub

## Unsub

We sending unsub command util received ack or switched to sub

## Sync

We sync each acked key-value or subscribe state in each interval time