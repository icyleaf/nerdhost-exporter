FROM --platform=$BUILDPLATFORM rust:1.93.0-alpine AS builder

ARG TARGETPLATFORM
RUN set -x \
    && apk add --no-cache build-base curl

RUN echo "Setting variables for ${TARGETPLATFORM}" && \
    case "$TARGETPLATFORM" in \
    "linux/amd64") \
        RUST_TARGET="x86_64-unknown-linux-musl" \
        MUSL="x86_64-linux-musl" \
    ;; \
    "linux/arm64") \
        RUST_TARGET="aarch64-unknown-linux-musl" \
        MUSL="aarch64-linux-musl" \
    ;; \
    "linux/arm/v7") \
        RUST_TARGET="armv7-unknown-linux-musleabi" \
        MUSL="armv7m-linux-musleabi" \
    ;; \
    "linux/riscv64") \
        RUST_TARGET="riscv64gc-unknown-linux-musl" \
        MUSL="riscv64-linux-musl" \
    ;; \
    *) \
        echo "Doesn't support $TARGETPLATFORM"; \
        exit 1 \
    ;; \
    esac && \
    curl -fsSL --max-time 180 -o /tmp/musl-cross.tgz "https://musl.cc/$MUSL-cross.tgz" && \
    echo "downloaded musl cross toolchain bytes: $(wc -c < /tmp/musl-cross.tgz)" && \
    tar -xzf /tmp/musl-cross.tgz -C /opt/ && \
    rustup target add "$RUST_TARGET" && \
    echo "$MUSL" > /tmp/musl && \
    echo "$RUST_TARGET" > /tmp/rusttarget

WORKDIR /workspace

COPY Cargo.lock ./Cargo.lock
COPY Cargo.toml ./Cargo.toml

COPY . .

RUN RUST_TARGET="$(cat /tmp/rusttarget)" && \
    export PATH="/opt/$(cat /tmp/musl)-cross/bin:$PATH" && \
    export CC="$(cat /tmp/musl)-gcc" && \
    export RUSTFLAGS="-C linker=$CC" && \
    mkdir -p target/release && \
    cargo build --target "$RUST_TARGET" --release && \
    mv target/$RUST_TARGET/release/racknerd-exporter target/release/

FROM alpine:3.24.2
RUN apk add --no-cache --update tini

COPY --from=builder /workspace/target/release/racknerd-exporter /usr/bin/racknerd-exporter

EXPOSE 9103/tcp

ENTRYPOINT ["/sbin/tini", "--", "/usr/bin/racknerd-exporter"]
CMD [ "--help" ]
