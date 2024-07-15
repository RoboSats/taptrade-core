#!/bin/bash

# Function to display help message
show_help() {
    echo "Usage: $0 [OPTIONS] <bitcoin_address>"
    echo
    echo "This script generates blocks to a specified Bitcoin address using RPC."
    echo
    echo "Options:"
    echo "  -h, --help    Show this help message and exit"
    echo
    echo "Arguments:"
    echo "  <bitcoin_address>    The Bitcoin address to generate blocks to"
    echo
    echo "Example:"
    echo "  $0 bcrt1pcc5nx64a9d6rpk5fkvr6v2lnk06cwxqmgpv3894ehgwkeeal2qusjgjrk3"
}

# Check for help option
if [[ "$1" == "-h" || "$1" == "--help" ]]; then
    show_help
    exit 0
fi

# Check if an address is provided
if [ $# -eq 0 ]; then
    echo "Error: Bitcoin address is required."
    echo "Use '$0 --help' for more information."
    exit 1
fi

# Get the Bitcoin address from command line argument
bitcoin_address="$1"

# Run the curl command with the provided Bitcoin address
curl --data-binary "{\"jsonrpc\":\"1.0\",\"id\":\"curltext\",\"method\":\"generatetoaddress\",\"params\":[101, \"$bitcoin_address\"]}" \
     -H 'content-type:text/plain;' \
     http://coordinator:test1234@127.0.0.1:8332/