#!/usr/bin/env sh

set -e

cargo build

export HOSTNAME=local-dev
export NODE_NAME=k3d-k3s-default-agent-0
export RUST_BACKTRACE=full
exec sudo -E ./target/debug/btrfs-provisioner "$@"