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
