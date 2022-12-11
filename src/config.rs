use lazy_static::lazy_static;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const STORAGE_CLASS_CONTROLLING_NODE_LABEL_NAME: &str = "btrfs-provisioner.timo.schwarzer.dev/node";
pub const PROVISIONED_BY_ANNOTATION_KEY: &str = "pv.kubernetes.io/provisioned-by";
pub const PROVISIONER_NAME: &str = "timo.schwarzer.dev/btrfs-provisioner";
pub const FINALIZER_NAME: &str = "timo.schwarzer.dev/btrfs-provisioner";
pub const NODE_HOSTNAME_KEY: &str = "kubernetes.io/hostname";
pub const SERVICE_ACCOUNT_NAME: &str = "btrfs-provisioner-service-account";
pub const HOST_FS_ENV_NAME: &str = "HOST_FS";

lazy_static! {
    pub static ref NAMESPACE: String = std::env::var("NAMESPACE").unwrap_or_else(|_| "btrfs-provisioner".into());
    pub static ref VOLUMES_DIR: String = std::env::var("VOLUMES_DIR").unwrap_or_else(|_| "/volumes".into());
    pub static ref IMAGE: String = std::env::var("IMAGE").unwrap_or_else(|_| "ghcr.io/timoschwarzer/btrfs-provisioner".into());
    pub static ref ARCHIVE_ON_DELETE: bool = matches!(std::env::var("ARCHIVE_ON_DELETE").unwrap_or_else(|_| "false".into()).as_str(), "true" | "1");
    pub static ref DYNAMIC_STORAGE_CLASS_NAME: String = std::env::var("DYNAMIC_STORAGE_CLASS_NAME").unwrap_or_else(|_| "btrfs-provisioner".into());
    pub static ref STORAGE_CLASS_NAME_PATTERN: String = {
        let pattern = std::env::var("STORAGE_CLASS_NAME_PATTERN").unwrap_or_else(|_| "btrfs-provisioner-{}".into());
        assert!(pattern.contains("{}"), "STORAGE_CLASS_NAME_PATTERN must contain a {{}} placeholder");
        pattern
    };
    pub static ref STORAGE_CLASS_PER_NODE: bool = matches!(std::env::var("STORAGE_CLASS_PER_NODE").unwrap_or_else(|_| "true".into()).as_str(), "true" | "1");
}

// Job labeling
pub const JOB_TYPE_LABEL: &str = "btrfs-provisioner.timo.schwarzer.dev/job-type";
pub const JOB_TYPE_PROVISION_VALUE: &str = "provision";
pub const JOB_TYPE_DELETE_VALUE: &str = "delete";
pub const JOB_TYPE_INITIALIZE_NODE_VALUE: &str = "initialize-node";
pub const JOB_TARGET_UID_LABEL: &str = "btrfs-provisioner.timo.schwarzer.dev/target-uid";
