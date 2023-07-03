# Omnisat Indexer (rust)

# Deployment on VM:
## Compress code files for transfer to VM
```bash
$ tar -zcvf omnisat-indexer-rs.tar.gz Cargo.toml entrypoint.sh Dockerfile src 
```
## Prepare remote directory where to trasnfer the source code to
SSH into the remote host
```bash
$ gcloud compute ssh indexer-omnisat-rs --zone us-central1-c --project pineappleworkshop
```
Make the directory example:
```bash
$ mkdir -p /home/zuwuko/omnisat-indexer-mongo-indexing
```
## send tarball to VM server
```bash
$ gcloud compute scp omnisat-indexer-rs.tar.gz indexer-omnisat-rs:/home/zuwuko/omnisat-indexer-mongo-indexing/ --zone us-central1-c --project pineappleworkshop
```
## Extract the codebase files
```bash
$ tar -zxvf omnisat-indexer-rs.tar.gz
```
## Verify .env file has values specified in the .env.example file as template

## Docker Build
```bash
$ sudo docker build -t gcr.io/pineappleworkshop/omnisat-indexer-rs-mongo-indexing:0.0.1 .
```
## Docker Run in background
```bash
$ sudo docker run -d --env-file .env --restart always gcr.io/pineappleworkshop/omnisat-indexer-rs-mongo-indexing:0.0.1
```
## See logs remotely:
```bash
$ gcloud compute ssh indexer-omnisat-rs --zone us-central1-c --project pineappleworkshop --command 'sudo docker logs -f 168e4236429c'
```

### getting started

#### pre-requisites
* full BTC node (unparsed -- started with `bitcoind --txindex`) running locally
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

