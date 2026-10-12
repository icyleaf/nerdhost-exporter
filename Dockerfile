FROM rust:1.93.0-alpine AS builder

RUN apk add --no-cache build-base

WORKDIR /workspace

COPY . .

RUN cargo build --release

FROM alpine:3.24.2
RUN apk add --no-cache --update tini

COPY --from=builder /workspace/target/release/racknerd-exporter /usr/bin/racknerd-exporter

EXPOSE 9103/tcp

ENTRYPOINT ["/sbin/tini", "--", "/usr/bin/racknerd-exporter"]
CMD [ "--help" ]
