# btrfs-provisioner for Kubernetes

This is a (work in progress) volume provisioner for Kubernetes leveraging the BTRFS filesystem to create volumes, enforce storage quotas, create snapshots and make backups.


### What works…

- Volume provisioning
- Volume deletion
- Enforcing storage quotas


### …and what doesn't (yet)

- Volume snapshots
- Volume backups using [Borg Backup](https://www.borgbackup.org/)


## Getting started


### Prerequisites

- A running K8s cluster
- A BTRFS directory or filesystem at `/volumes`.


### Installation

soon™