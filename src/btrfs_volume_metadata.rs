use std::path::{PathBuf};
use color_eyre::Result;
use crate::config::*;
use crate::provisioner::Provisioner;

pub struct BtrfsVolumeMetadata {
    pub path: PathBuf,
    pub host_path: PathBuf,
}

impl BtrfsVolumeMetadata {
    pub fn from_pv_name(pv_name: &str) -> Result<BtrfsVolumeMetadata> {
        let path_parts = vec![VOLUMES_DIR, pv_name];

        let path: PathBuf = path_parts.iter().collect();
        let host_path = Provisioner::get_host_path(&path_parts)?;

        Ok(BtrfsVolumeMetadata {
            path,
            host_path,
        })
    }
}