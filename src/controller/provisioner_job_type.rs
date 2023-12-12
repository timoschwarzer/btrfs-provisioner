use crate::config::*;
use color_eyre::eyre::{bail, eyre};
use color_eyre::Result;
use std::collections::BTreeMap;

pub struct ProvisionJobArgs {
    pub target_pvc_uid: String,
}

pub struct DeleteJobArgs {
    pub target_pv_uid: String,
}

pub struct InitializeNodeJobArgs {
    pub target_node_uid: String,
}

pub enum ProvisionerJobType {
    Provision(ProvisionJobArgs),
    Delete(DeleteJobArgs),
    InitializeNode(InitializeNodeJobArgs),
}

impl ProvisionerJobType {
    pub fn from_labels(labels: BTreeMap<String, String>) -> Result<ProvisionerJobType> {
        if !labels.contains_key(JOB_TYPE_LABEL) {
            bail!("Labels didn't contain required label {}", JOB_TYPE_LABEL);
        }

        match labels.get(JOB_TYPE_LABEL).unwrap().as_str() {
            JOB_TYPE_PROVISION_VALUE => Ok(ProvisionerJobType::Provision(ProvisionJobArgs {
                target_pvc_uid: labels
                    .get(JOB_TARGET_UID_LABEL)
                    .ok_or_else(|| {
                        eyre!(
                            "Required label {} missing for type={}",
                            JOB_TARGET_UID_LABEL,
                            JOB_TYPE_PROVISION_VALUE
                        )
                    })?
                    .to_owned(),
            })),
            JOB_TYPE_DELETE_VALUE => Ok(ProvisionerJobType::Delete(DeleteJobArgs {
                target_pv_uid: labels
                    .get(JOB_TARGET_UID_LABEL)
                    .ok_or_else(|| {
                        eyre!(
                            "Required label {} missing for type={}",
                            JOB_TARGET_UID_LABEL,
                            JOB_TYPE_DELETE_VALUE
                        )
                    })?
                    .to_owned(),
            })),
            JOB_TYPE_INITIALIZE_NODE_VALUE => {
                Ok(ProvisionerJobType::InitializeNode(InitializeNodeJobArgs {
                    target_node_uid: labels
                        .get(JOB_TARGET_UID_LABEL)
                        .ok_or_else(|| {
                            eyre!(
                                "Required label {} missing for type={}",
                                JOB_TARGET_UID_LABEL,
                                JOB_TYPE_INITIALIZE_NODE_VALUE
                            )
                        })?
                        .to_owned(),
                }))
            }
            other_job_type => bail!("Invalid job type: {}", other_job_type),
        }
    }

    pub fn to_labels(&self) -> BTreeMap<String, String> {
        let mut labels = BTreeMap::<String, String>::new();

        match self {
            ProvisionerJobType::Provision(args) => {
                labels.insert(JOB_TYPE_LABEL.into(), JOB_TYPE_PROVISION_VALUE.into());
                labels.insert(JOB_TARGET_UID_LABEL.into(), args.target_pvc_uid.to_owned());
            }
            ProvisionerJobType::Delete(args) => {
                labels.insert(JOB_TYPE_LABEL.into(), JOB_TYPE_DELETE_VALUE.into());
                labels.insert(JOB_TARGET_UID_LABEL.into(), args.target_pv_uid.to_owned());
            }
            ProvisionerJobType::InitializeNode(args) => {
                labels.insert(JOB_TYPE_LABEL.into(), JOB_TYPE_DELETE_VALUE.into());
                labels.insert(JOB_TARGET_UID_LABEL.into(), args.target_node_uid.to_owned());
            }
        }

        labels
    }

    pub fn to_label_selector(&self) -> String {
        let labels = self.to_labels();

        let label_strings: Vec<String> = labels
            .iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect();

        label_strings.join(",")
    }
}
