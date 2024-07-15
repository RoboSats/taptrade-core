FROM debian:latest

RUN apt update

RUN mkdir -p /home/fulcrum

COPY ./fulcrum /home/fulcrum/.
COPY ./config.conf /home/fulcrum/.

WORKDIR /home/fulcrum

RUN mkdir fulcrum_data
RUN chmod +x ./fulcrum

EXPOSE 5321

#CMD ["tail", "-f"]
CMD ["./fulcrum", "config.conf"]
