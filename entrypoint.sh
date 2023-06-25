#!/bin/bash
echo "RPC_URL="$RPC_URL >> .env
echo "RPC_USER="$RPC_USER >> .env
echo "RPC_PASSWORD="$RPC_PASSWORD >> .env
cargo run
# sleep 30000