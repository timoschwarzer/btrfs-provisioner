# btrfs-provisioner for Kubernetes

This is a (work in progress) volume provisioner for Kubernetes leveraging the BTRFS filesystem to create volumes, enforce storage quotas, create snapshots and make backups.


### What works…

- Volume provisioning
- Volume deletion
- Enforcing storage quotas
- Static (per Node) StorageClasses


### …and what doesn't (yet)

- Volume snapshots
- Volume backups using [Borg Backup](https://www.borgbackup.org/)
- Dynamic StorageClass


## Getting started


### Prerequisites

- A running K8s cluster
- A BTRFS directory or filesystem at `/volumes`.


### Installation

There are only plain Kubernetes manifests yet. A Helm chart should come soon when the provisioner reaches a stable state.

To deploy and test the BTRFS provisioner in your cluster, deploy the manifests in the `deploy` directory:

```shell
kubectl apply -f deploy/meta.yaml
kubectl apply -f deploy/controller.yaml
```

The BTRFS provisioner controller creates a StorageClass for each worker node on startup.