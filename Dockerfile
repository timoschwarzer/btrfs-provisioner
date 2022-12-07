FROM rust as build

ENV CARGO_TARGET_DIR=/build
RUN apt-get update -y && \
    apt-get install -y libssl-dev

ARG CARGO_PROFILE=release
WORKDIR /app
COPY . /app

RUN --mount=type=cache,target=/build \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --profile $CARGO_PROFILE && \
    mkdir -p /output && \
    cp /build/*/btrfs-provisioner /output/btrfs-provisioner


FROM debian:11-slim

ENV RUST_BACKTRACE=full

COPY --from=build /output/btrfs-provisioner /app/btrfs-provisioner

ENTRYPOINT ["/app/btrfs-provisioner"]