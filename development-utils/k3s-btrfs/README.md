# Run a local cluster with k3d and btrfs support


## 1. Build k3s with btrfs support

```shell
docker build -t k3s-btrfs .
```


## 2. Create a local cluster


### 2.a On a host that already runs on BTRFS

```shell
k3d cluster create -i k3s-btrfs --no-lb -a 1 -s 1 --k3s-arg "--disable=local-storage@server:0"
```


### 2.b On a host without BTRFS

1. Install BTRFS tools on your computer
2. Create a BTRFS filesystem on a new partition
3. Mount that filesystem to `/btrfs_vol`

```shell
k3d cluster create -i k3s-btrfs --no-lb -a 1 -s 1 --k3s-arg "--disable=local-storage@server:0" -v "/btrfs_vol:/volumes@agent:0"
```