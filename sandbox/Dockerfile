FROM alpine:latest
RUN adduser -D user
RUN apk add gcc g++ libc-dev python3 openjdk17-jdk rustup
RUN python3 -m ensurepip
RUN pip3 install cbor2
USER user
RUN rustup-init -y --profile minimal
USER root
COPY run.py /run.py
WORKDIR /tmp
ENTRYPOINT cp /input/command command; chown user command; umount /input; su user -c 'source ~/.cargo/env; python3 /run.py'
