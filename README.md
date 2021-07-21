# Distributed-Cache
Created a in memory key value store, using two strategies:
1. Replication using Raft
2. Error Correcting codes using Reed Solomon

## Assumptions
- clients are healthy for the duration of a transacation

## Todo overall
- error handling 
- integration tests

## Todo replication
- client
- Storage hard state

## Todo ecc
- optomistic concurrency writes
- restore (no in flight transactions?)
- better timeout