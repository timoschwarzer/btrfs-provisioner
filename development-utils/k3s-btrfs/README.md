# Run a local cluster with k3d and btrfs support


## 1. Build k3s with btrfs support

```shell
docker build -t k3s-btrfs .
```


## 2. Create a local cluster

```shell
k3d cluster create -i k3s-btrfs --no-lb -a 1 -s 1 --k3s-arg "--disable=local-storage@server:0"
```