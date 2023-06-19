# Omnisat Indexer (rust)


### getting started

#### pre-requisites
* full BTC node (unparsed -- start with `bitcoind --txindex) running locally
* rust

start node with `bitcoin.conf` settings as such:
```shell
$ cat <PATH TO BITCOIN INSTALL>/Bitcoin/bitcoin.conf
server=1
txindex=1
listen=0
rpcport=8333
rpcuser=<MAKE A USERNAME>
rpcpassword=<MAKE A PASSWORD>
```


```shell
$ cat .env
RPC_USER=<SAME USERNAME AS bitcoin.conf>
RPC_PASSWORD=<SAME PASSWORD AS bitcoin.conf>
RPC_URL=127.0.0.1:8333
```
with all of that in place, you should be good to geaux

```shell
cargo run
```

