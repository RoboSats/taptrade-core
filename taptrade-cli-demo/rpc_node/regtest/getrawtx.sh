#!/bin/bash

# Check if an address is provided
if [ $# -eq 0 ]; then
    echo "Error: Raw hex tx is required."
    exit 1
fi

# Get the Bitcoin address from command line argument
tx="$1"

curl --data-binary "{\"jsonrpc\":\"1.0\",\"id\":\"curltext\",\"method\":\"getrawtransaction\",\"params\":[\"$tx\"]}" \
     -H 'content-type:text/plain;' \
     http://coordinator:test1234@127.0.0.1:8332 | \
     jq '.result'
