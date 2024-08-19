#!/bin/sh

/entrypoint.sh bitcoind -regtest -daemon
sleep 10;

# List existing wallets
WALLETS=$(bitcoin-cli -regtest -datadir="/home/bitcoin/.bitcoin" listwalletdir)

# Check if "coordinator_wallet" exists in the list of wallets
if echo "$WALLETS" | grep -q "coordinator_wallet"; then
    echo "Wallet exists. Loading wallet..."
    bitcoin-cli -regtest -datadir="/home/bitcoin/.bitcoin" loadwallet "coordinator_wallet"
else
    echo "Wallet does not exist. Creating wallet..."
    bitcoin-cli -regtest -datadir="/home/bitcoin/.bitcoin" createwallet "coordinator_wallet"
    # Generate initial blocks
    bitcoin-cli -regtest -datadir="/home/bitcoin/.bitcoin" -rpcwallet="coordinator_wallet" -generate 101
fi

# Generate a block every 10 seconds
while true; do
    bitcoin-cli -regtest -datadir="/home/bitcoin/.bitcoin" -rpcwallet="coordinator_wallet" -generate 1
    sleep 10
done
