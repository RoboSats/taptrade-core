FROM debian:bookworm

RUN apt update && apt install -y git bash vim clang cmake build-essential librocksdb-dev=7.8.3-2  cargo rustc

RUN mkdir -p /home/electrs

WORKDIR /home/electrs

RUN git clone https://github.com/romanz/electrs --branch v0.10.5 --single-branch

WORKDIR /home/electrs/electrs

RUN ROCKSDB_INCLUDE_DIR=/usr/include ROCKSDB_LIB_DIR=/usr/lib cargo build --locked --release --no-default-features

RUN mv /home/electrs/electrs/target/release/electrs /usr/local/bin

WORKDIR /home/electrs

RUN rm -rf /home/electrs/electrs

RUN mkdir db

CMD ["electrs"]