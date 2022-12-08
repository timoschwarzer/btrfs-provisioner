use lazy_static::lazy_static;

pub const STORAGE_CLASS_NAME: &str = "btrfs-provisioner";
pub const PROVISIONED_BY_ANNOTATION_KEY: &str = "pv.kubernetes.io/provisioned-by";
pub const PROVISIONER_NAME: &str = "timo.schwarzer.dev/btrfs-provisioner";
pub const FINALIZER_NAME: &str = "timo.schwarzer.dev/btrfs-provisioner";
pub const NODE_ANNOTATION_NAME: &str = "btrfs-provisioner.timo.schwarzer.dev/node";
pub const NODE_HOSTNAME_KEY: &str = "kubernetes.io/hostname";
pub const SERVICE_ACCOUNT_NAME: &str = "btrfs-provisioner-service-account";
pub const HOST_FS_ENV_NAME: &str = "HOST_FS";

lazy_static! {
    pub static ref NAMESPACE: String = std::env::var("NAMESPACE").unwrap_or_else(|_| "btrfs-provisioner".into());
    pub static ref VOLUMES_DIR: String = std::env::var("VOLUMES_DIR").unwrap_or_else(|_| "/volumes".into());
    pub static ref IMAGE: String = std::env::var("IMAGE").unwrap_or_else(|_| "ghcr.io/timoschwarzer/btrfs-provisioner".into());
}

// Job labeling
pub const JOB_TYPE_LABEL: &str = "btrfs-provisioner.timo.schwarzer.dev/job-type";
pub const JOB_TYPE_PROVISION_VALUE: &str = "provision";
pub const JOB_TYPE_DELETE_VALUE: &str = "delete";
pub const JOB_TARGET_UID_LABEL: &str = "btrfs-provisioner.timo.schwarzer.dev/target-uid";
