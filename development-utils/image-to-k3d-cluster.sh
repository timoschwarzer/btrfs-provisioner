#!/usr/bin/env sh

set -e

DOCKER_BUILDKIT=true docker build --build-arg=CARGO_PROFILE=dev -t timoschwarzer/btrfs-provisioner .
k3d image import -m direct timoschwarzer/btrfs-provisioner