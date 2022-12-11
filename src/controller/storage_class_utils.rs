use k8s_openapi::api::storage::v1::StorageClass;
use kube::{Api, Client, ResourceExt};
use color_eyre::Result;
use crate::config::*;

pub trait StorageClassExt {
    /// Returns whether this StorageClass is managed by btrfs-provisioner
    fn is_controlling(&self) -> bool;

    /// Returns the node name this StorageClass should schedule to, determined by [STORAGE_CLASS_CONTROLLING_NODE_LABEL_NAME]
    fn get_controlling_node_name(&self) -> Option<&String>;
}

impl StorageClassExt for StorageClass {
    fn is_controlling(&self) -> bool {
        self.provisioner == PROVISIONER_NAME
    }

    fn get_controlling_node_name(&self) -> Option<&String> {
        self
            .metadata
            .labels.as_ref()?
            .get(STORAGE_CLASS_CONTROLLING_NODE_LABEL_NAME)
    }
}

async fn get_storage_class_by_name(client: Client, name: &str) -> Result<Option<StorageClass>> {
    let storage_classes = Api::<StorageClass>::all(client);

    if let Some(storage_class) = storage_classes.get_opt(name).await? {
        return Ok(Some(storage_class))
    } else {
        eprintln!("Storage class '{}' not found", name);
    }

    Ok(None)
}

/// Returns whether the StorageClass called `name` is managed by btrfs-provisioner
pub async fn is_controlling_storage_class(client: Client, name: &str) -> Result<bool> {
    let storage_class = get_storage_class_by_name(client, name).await?;

    if let Some(storage_class) = storage_class {
        return Ok(storage_class.is_controlling());
    }

    Ok(false)
}

/// Returns whether a StorageClass called `name` is controlled by Node `node`
pub async fn node_can_control_storage_class(client: Client, storage_class_name: &str, node_name: &str) -> Result<bool> {
    let storage_class = get_storage_class_by_name(client, storage_class_name).await?;

    if let Some(storage_class) = storage_class {
        if !storage_class.is_controlling() {
            return Ok(false)
        }

        if let Some(assigned_node) = storage_class.get_controlling_node_name() {
            return Ok(assigned_node == "*" || assigned_node == node_name);
        } else {
            eprintln!("StorageClass does not have required annotation {}: {}", STORAGE_CLASS_CONTROLLING_NODE_LABEL_NAME, storage_class.name_any());
        }
    }

    Ok(false)
}

pub enum StorageClassNodeAssignment {
    SingleNode { node_name: String },
    Dynamic,
}

impl StorageClassNodeAssignment {
    pub fn from_string(controlling_node_name: &str) -> StorageClassNodeAssignment {
        match controlling_node_name {
            "*" => StorageClassNodeAssignment::Dynamic,
            node_name => StorageClassNodeAssignment::SingleNode { node_name: node_name.to_owned() }
        }
    }
}

pub async fn get_node_assigned_to_storage_class(client: Client, storage_class_name: &str) -> Result<Option<StorageClassNodeAssignment>> {
    let storage_class = get_storage_class_by_name(client, storage_class_name).await?;

    if let Some(storage_class) = storage_class {
        return Ok(
            storage_class
                .get_controlling_node_name()
                .map_or(None, |node_name| Some(StorageClassNodeAssignment::from_string(node_name)))
        );
    }

    Ok(None)
}