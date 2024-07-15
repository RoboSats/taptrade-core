FROM dobtc/bitcoin

COPY ./bitcoin.conf /home/bitcoin/.bitcoin/bitcoin.conf

WORKDIR /home/bitcoin
COPY ./mine-blocks.sh ./mine-blocks.sh
RUN chmod +x ./mine-blocks.sh

CMD ["./mine-blocks.sh"]