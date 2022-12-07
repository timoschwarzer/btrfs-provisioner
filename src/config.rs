pub const VOLUMES_DIR: &str = "/volumes";
pub const STORAGE_CLASS_NAME: &str = "btrfs-provisioner";
pub const NAMESPACE: &str = "btrfs-provisioner";
pub const PROVISIONED_BY_ANNOTATION_KEY: &str = "pv.kubernetes.io/provisioned-by";
pub const PROVISIONER_NAME: &str = "timo.schwarzer.dev/btrfs-provisioner";
pub const FINALIZER_NAME: &str = "timo.schwarzer.dev/btrfs-provisioner";
pub const NODE_ANNOTATION_NAME: &str = "btrfs-provisioner.timo.schwarzer.dev/node";
pub const NODE_HOSTNAME_KEY: &str = "kubernetes.io/hostname";
pub const IMAGE: &str = "timoschwarzer/btrfs-provisioner";
pub const SERVICE_ACCOUNT_NAME: &str = "btrfs-provisioner-service-account";
pub const HOST_FS_ENV_NAME: &str = "HOST_FS";