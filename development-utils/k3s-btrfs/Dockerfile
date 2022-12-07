FROM alpine as alpine-btrfs

RUN apk add --no-cache btrfs-progs


FROM rancher/k3s:v1.25.4-k3s1

COPY --from=alpine-btrfs /sbin/btrfs /bin/btrfs
COPY --from=alpine-btrfs /lib/ld-musl-x86_64.so.1 /lib/ld-musl-x86_64.so.1
COPY --from=alpine-btrfs /lib/libc.musl-x86_64.so.1 /lib/libc.musl-x86_64.so.1
COPY --from=alpine-btrfs /usr/lib/libzstd.so.1 /usr/lib/libzstd.so.1
COPY --from=alpine-btrfs /lib/libz.so.1 /lib/libz.so.1
COPY --from=alpine-btrfs /usr/lib/liblzo2.so.2 /usr/lib/liblzo2.so.2
COPY --from=alpine-btrfs /lib/libuuid.so.1 /lib/libuuid.so.1
COPY --from=alpine-btrfs /lib/libblkid.so.1 /lib/libblkid.so.1
COPY --from=alpine-btrfs /lib/libudev.so.1 /lib/libudev.so.1