FROM rust:1.60 as builder
WORKDIR /usr/src/binaries
COPY . .
RUN cargo install --path .

FROM ubuntu:20.04
RUN apt-get update && apt-get install -y openssl ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/binaries /usr/local/bin/binaries
ENTRYPOINT [ "binaries" ]
# RUN update-ca-certificates
# RUN /bin/cp /usr/share/zoneinfo/Asia/Shanghai /etc/localtime && echo 'Asia/Shanghai' >/etc/timezone
# COPY ./target/release/binaries /usr/local/bin/binaries
# # COPY --from=builder /usr/local/cargo/bin/binaries /usr/local/bin/binaries
# COPY ./config.yaml /config.yaml
# RUN useradd -ms /bin/bash newuser
# USER newuser
# WORKDIR /home/newuser
# RUN date; binaries -vvvv -f /config.yaml install
# CMD ["sh", "-c", "binaries -vvvv -f /config.yaml install"]
