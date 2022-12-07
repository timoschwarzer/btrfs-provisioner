use std::collections::{BTreeMap};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use color_eyre::eyre::{bail, eyre};
use color_eyre::Result;
use k8s_openapi::api::core::v1::{HostPathVolumeSource, NodeSelector, NodeSelectorRequirement, NodeSelectorTerm, PersistentVolume, PersistentVolumeClaim, PersistentVolumeClaimSpec, PersistentVolumeSpec, ResourceRequirements, VolumeNodeAffinity};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::{Api, Client, Config, Resource, ResourceExt};
use kube::api::PostParams;
use kube::api::entry::Entry;
use mkdirp::mkdirp;
use rand::{Rng, thread_rng};
use rand::distributions::Alphanumeric;

use crate::config::*;
use crate::ext::ProvisionerResourceExt;
use crate::quantity_parser::QuantityParser;

pub struct Provisioner {
    client: Client,
    node_name: String,
}

impl Provisioner {
    pub async fn create(node_name: String) -> Result<Self> {
        let client = Client::try_default()
            .await
            .or_else(|_| Client::try_from(Config::incluster_env().expect("Failed to load in-cluster Kube config")))
            .expect("Failed to create Kube client");

        Ok(Provisioner {
            client,
            node_name,
        })
    }

    pub async fn provision_persistent_volume_by_claim_name(&self, claim_namespace: &str, claim_name: &str) -> Result<()> {
        let client = self.client();

        let persistent_volume_claims = Api::<PersistentVolumeClaim>::namespaced(client.clone(), claim_namespace);

        let claim = persistent_volume_claims.get(claim_name).await?;
        self.provision_persistent_volume(&claim).await
    }

    pub async fn provision_persistent_volume(&self, claim: &PersistentVolumeClaim) -> Result<()> {
        Provisioner::prepare_directories()?;
        let client = self.client();

        let persistent_volumes = Api::<PersistentVolume>::all(client);

        // Check that the PVC has a storage request
        if let PersistentVolumeClaim {
            spec: Some(
                PersistentVolumeClaimSpec {
                    resources: Some(
                        ResourceRequirements {
                            requests: Some(requests), ..
                        }
                    ), ..
                }
            ), ..
        } = &claim {
            let storage_request = requests.get("storage").ok_or_else(|| eyre!("PVC {} does not have a storage request", claim.full_name()))?;
            let storage_request_bytes = storage_request.to_bytes()?.ok_or_else(|| eyre!("Failed to parse storage request: '{}'", storage_request.0))?;

            println!("Provisioning claim {}", claim.full_name());
            let pv_name = self.generate_pv_name_for_claim(claim).await?;

            let volume_path: PathBuf = [VOLUMES_DIR, &pv_name].iter().collect();
            let volume_host_path = Provisioner::get_host_path(&[VOLUMES_DIR, &pv_name])?;
            let volume_path_str = volume_path.to_str().ok_or_else(|| eyre!("Failed to convert path to string"))?;

            if !Provisioner::get_host_path(&[VOLUMES_DIR])?.exists() {
                bail!("The root volumes directory at {} does not exist. Please create it or mount a btrfs filesystem yourself.", VOLUMES_DIR);
            }

            println!("Creating btrfs subvolume at {}", volume_path_str);
            if volume_host_path.exists() {
                bail!("Cannot create btrfs subvolume, file/directory exists!");
            }
            self.run_btrfs_command_on_host(&["subvolume", "create", volume_path_str])?;

            println!("Enabling Quota on {}", volume_path_str);
            self.run_btrfs_command_on_host(&["quota", "enable", volume_path_str])?;

            println!("Setting Quota limit on {} to {} bytes", volume_path_str, storage_request_bytes);
            self.run_btrfs_command_on_host(&["qgroup", "limit", storage_request_bytes.to_string().as_str(), volume_path_str])?;

            println!("Triggering subvolume rescan");
            self.run_btrfs_command_on_host(&["quota", "rescan", volume_path_str])?;

            println!("Creating PersistentVolume {}", pv_name);
            let mut annotations: BTreeMap<String, String> = BTreeMap::new();
            annotations.insert(PROVISIONED_BY_ANNOTATION_KEY.into(), PROVISIONER_NAME.into());

            persistent_volumes.create(&PostParams::default(), &PersistentVolume {
                metadata: ObjectMeta {
                    labels: Some(claim.labels().clone()),
                    annotations: Some(annotations),
                    name: Some(pv_name.clone()),
                    finalizers: Some(vec![FINALIZER_NAME.into()]),
                    ..Default::default()
                },
                spec: Some(PersistentVolumeSpec {
                    host_path: Some(HostPathVolumeSource {
                        path: volume_path_str.into(),
                        ..Default::default()
                    }),
                    claim_ref: Some(claim.object_ref(&())),
                    access_modes: Some(vec![String::from("ReadWriteOnce")]),
                    capacity: Some(requests.clone()),
                    storage_class_name: Some(STORAGE_CLASS_NAME.into()),
                    node_affinity: Some(VolumeNodeAffinity {
                        required: Some(NodeSelector {
                            node_selector_terms: vec![NodeSelectorTerm {
                                match_expressions: Some(vec![NodeSelectorRequirement {
                                    key: NODE_HOSTNAME_KEY.into(),
                                    operator: "In".into(),
                                    values: Some(vec![self.node_name.to_owned()]),
                                }]),
                                ..Default::default()
                            }]
                        })
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }).await?;

            println!("Created volume {}", pv_name);
        } else {
            bail!("PVC {} does not have resource requests", claim.full_name());
        }

        Ok(())
    }

    fn client(&self) -> Client {
        self.client.clone()
    }

    async fn generate_pv_name_for_claim(&self, claim: &PersistentVolumeClaim) -> Result<String> {
        let client = self.client();

        let persistent_volumes = Api::<PersistentVolume>::all(client);

        loop {
            let rand_string: String = thread_rng()
                .sample_iter(&Alphanumeric)
                .take(5)
                .map(|u| char::from(u).to_ascii_lowercase())
                .collect();

            let generated_name = format!("{}-{}-{}", claim.namespace().unwrap_or_else(|| "default".into()), claim.name_any(), rand_string);

            if let Entry::Vacant(_) = persistent_volumes.entry(&generated_name).await? {
                return Ok(generated_name);
            }
        }
    }

    fn prepare_directories() -> Result<()> {
        match mkdirp(VOLUMES_DIR) {
            Err(e) => panic!("Error while creating volume directory at {}: {}", VOLUMES_DIR, e),
            Ok(_) => Ok(())
        }
    }

    fn run_btrfs_command_on_host(&self, args: &[&str]) -> Result<Output> {
        fn run_command(command: &mut Command, args: &[&str]) -> Result<Output> {
            println!("Running: {:?}", command);

            let output = &command
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .output()?;

            if !&output.status.success() {
                bail!("`btrfs {}` failed: {}", &args.join(" "), &output.status);
            }

            Ok(output.clone())
        }

        match std::env::var(HOST_FS_ENV_NAME) {
            Ok(path) => {
                run_command(
                    Command::new("chroot")
                        .args(vec![path.as_str(), "btrfs"])
                        .args(args),
                    args,
                )
            }
            Err(_) => {
                run_command(
                    Command::new("btrfs")
                        .args(args),
                    args,
                )
            }
        }
    }

    fn get_host_path(path: &[&str]) -> Result<PathBuf> {
        let mut path_buf = PathBuf::new();

        if let Ok(path) = std::env::var(HOST_FS_ENV_NAME) {
            path_buf.push(path);
        }

        for part in path {
            path_buf.push(part.trim_start_matches('/'));
        }

        Ok(path_buf)
    }
}