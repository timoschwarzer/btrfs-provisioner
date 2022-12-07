#!/usr/bin/env sh

set -e

DOCKER_BUILDKIT=true docker build --build-arg=CARGO_PROFILE=dev -t ghcr.io/timoschwarzer/btrfs-provisioner .
k3d image import -m direct ghcr.io/timoschwarzer/btrfs-provisioner