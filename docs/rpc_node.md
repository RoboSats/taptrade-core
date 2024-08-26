# Regtest Node

This folder contains the documentation for setting up and using the Regtest Node in the TapTrade Core repository.

Any developer can use this to test this repository in an isolated Regtest node.

## Prerequisites

Before using the Regtest Node, make sure you have the following prerequisites installed:

- docker
- docker compose

## Installation

To install the Regtest Node, follow these steps:

1. Clone the TapTrade Core repository to your local machine.

### Build the Docker Containers:
1. Navigate to the `rpc_node` folder in the repository.
    ```
    cd rpc_node/regtest
    ```

2. Run the following command to build the services:
    ```
    docker compose build
    ```

### Start the Services:

1. Once the containers are built, you can start the Bitcoin and Electrs services:
    ```
    docker compose up -d
    ```
2. This command will run the containers in detached mode. To see the logs in real-time, use:
    ```
    docker compose logs -f
    ```
### Check If the Services Are Running:
    You can check the status of your containers by running:
    ```
    docker ps
    ```

## API Documentation

For detailed information on the available API endpoints and how to use them, refer to the [API documentation](./api_docs.md) file in this folder.