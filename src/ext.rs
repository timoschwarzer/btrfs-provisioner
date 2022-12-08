use std::path::PathBuf;
use color_eyre::eyre::eyre;
use color_eyre::Result;
use kube::ResourceExt;

pub trait ProvisionerResourceExt: ResourceExt {
    fn full_name(&self) -> String;
}

impl<K: ResourceExt> ProvisionerResourceExt for K {
    fn full_name(&self) -> String {
        format!(
            "{}/{}",
            self.namespace().unwrap_or_else(|| "<>".into()),
            self.name_any()
        )
    }
}

pub trait PathBufExt {
    fn as_str(&self) -> Result<&str>;
}

impl PathBufExt for PathBuf {
    fn as_str(&self) -> Result<&str> {
        return self.to_str().ok_or_else(|| eyre!("Could not convert path to string"))
    }
}