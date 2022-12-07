use std::collections::{BTreeMap, HashSet};
use std::env::VarError;
use std::path::{Path, PathBuf};
use color_eyre::Result;
use futures_util::{stream, StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::{Affinity, Container, EnvFromSource, EnvVar, EnvVarSource, HostPathVolumeSource, NodeAffinity, NodeSelector, NodeSelectorRequirement, NodeSelectorTerm, ObjectFieldSelector, PersistentVolume, PersistentVolumeClaim, PersistentVolumeClaimSpec, PersistentVolumeClaimStatus, PersistentVolumeSpec, Pod, PodSpec, ResourceRequirements, SecurityContext, Volume, VolumeMount, VolumeNodeAffinity};
use k8s_openapi::api::storage::v1::StorageClass;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::{Api, Client, Config, Resource, ResourceExt};
use kube::api::{ListParams, PostParams};
use kube::api::entry::Entry;
use kube::runtime::{reflector, watcher};
use kube::runtime::watcher::Event;
use mkdirp::mkdirp;
use rand::{thread_rng, Rng};
use rand::distributions::Alphanumeric;
use std::process::{Command, Output, Stdio};
use color_eyre::eyre::{bail, eyre};
use crate::ext::ProvisionerResourceExt;
use crate::quantity_parser::QuantityParser;

const VOLUMES_DIR: &str = "/volumes";
const STORAGE_CLASS_NAME: &str = "btrfs-provisioner";
const NAMESPACE: &str = "btrfs-provisioner";
const PROVISIONED_BY_ANNOTATION_KEY: &str = "pv.kubernetes.io/provisioned-by";
const PROVISIONER_NAME: &str = "timo.schwarzer.dev/btrfs-provisioner";
const FINALIZER_NAME: &str = "timo.schwarzer.dev/btrfs-provisioner";
const NODE_ANNOTATION_NAME: &str = "btrfs-provisioner.timo.schwarzer.dev/node";
const NODE_HOSTNAME_KEY: &str = "kubernetes.io/hostname";
const IMAGE: &str = "timoschwarzer/btrfs-provisioner";
const SERVICE_ACCOUNT_NAME: &str = "btrfs-provisioner-service-account";
const HOST_FS_ENV_NAME: &str = "HOST_FS";

pub struct Provisioner {
    client: Client,
    active_pvc_uids: HashSet<String>,
    active_pv_uids: HashSet<String>,
}

enum WatchedResource {
    Pv(Event<PersistentVolume>),
    Pvc(Event<PersistentVolumeClaim>),
}

impl Provisioner {
    pub async fn create() -> Result<Self> {
        let client = Client::try_default()
            .await
            .or_else(|_| Client::try_from(Config::incluster_env().expect("Failed to load in-cluster Kube config")))
            .expect("Failed to create Kube client");

        Ok(Provisioner {
            client,
            active_pvc_uids: HashSet::new(),
            active_pv_uids: HashSet::new(),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        Provisioner::prepare_directories()?;

        self.ensure_storage_class_exists().await?;

        println!("Provisioner started.");

        self.watch_persistent_volume_claims().await?;

        Ok(())
    }

    pub async fn provision_persistent_volume_by_claim_name(&self, claim_namespace: &str, claim_name: &str, node_name: &str) -> Result<()> {
        let client = self.client();

        let persistent_volume_claims = Api::<PersistentVolumeClaim>::namespaced(client.clone(), claim_namespace);

        let claim = persistent_volume_claims.get(claim_name).await?;
        self.provision_persistent_volume(&claim, node_name).await
    }

    pub async fn provision_persistent_volume(&self, claim: &PersistentVolumeClaim, node_name: &str) -> Result<()> {
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
                                    values: Some(vec![node_name.into()]),
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

    async fn watch_persistent_volume_claims(&mut self) -> Result<()> {
        let client = self.client();

        let persistent_volume_claims = Api::<PersistentVolumeClaim>::all(client.clone());
        let persistent_volumes = Api::<PersistentVolume>::all(client.clone());

        let (_pvc_reader, pvc_writer) = reflector::store();
        let (_pv_reader, pv_writer) = reflector::store();
        let pvc_reflector = reflector(pvc_writer, watcher(persistent_volume_claims, ListParams::default()))
            .map_ok(WatchedResource::Pvc);
        let pv_reflector = reflector(pv_writer, watcher(persistent_volumes, ListParams::default()))
            .map_ok(WatchedResource::Pv);

        let stream = stream::select_all(vec![pvc_reflector.boxed(), pv_reflector.boxed()]);

        tokio::pin!(stream);

        while let Ok(Some(watched_resource)) = stream.try_next().await {
            match watched_resource {
                WatchedResource::Pvc(pvc) => self.process_pvc_event(pvc).await?,
                WatchedResource::Pv(pv) => self.process_pv_event(pv).await?,
            }
        };

        Ok(())
    }

    async fn process_pvc_event(&mut self, event: Event<PersistentVolumeClaim>) -> Result<()> {
        for claim in event.into_iter_applied() {
            if let PersistentVolumeClaim { spec: Some(PersistentVolumeClaimSpec { storage_class_name: Some(storage_class_name), .. }), status: Some(PersistentVolumeClaimStatus { phase: Some(phase), .. }), .. } = &claim {
                if storage_class_name != STORAGE_CLASS_NAME {
                    // Ignore any PVCs not assigned to our storage class
                    continue;
                }

                match phase.as_str() {
                    "Pending" => {
                        if let Some(uid) = &claim.uid() {
                            if self.active_pvc_uids.contains(uid) {
                                continue;
                            }

                            println!("Pending: {}", &claim.full_name());
                            self.active_pvc_uids.insert(uid.clone());

                            let claim_namespace = &claim.namespace().unwrap();
                            let claim_name = &claim.name_any();

                            let annotations = &claim.metadata.annotations.unwrap_or_default();
                            if !&annotations.contains_key(NODE_ANNOTATION_NAME) {
                                eprintln!("PVC does not have required annotation {}", NODE_ANNOTATION_NAME);
                                continue;
                            }

                            let node_name = annotations.get(NODE_ANNOTATION_NAME).unwrap();

                            println!("Starting volume provisioning helper pod on Node {}", node_name);
                            self.deploy_helper_pod("provision-volume", node_name, &["provision", claim_namespace, claim_name]).await?;
                        }
                    }
                    "Bound" => {
                        if let Some(uid) = &claim.uid() {
                            if self.active_pvc_uids.contains(uid) {
                                continue;
                            }

                            println!("Bound: {}", &claim.full_name());
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    async fn process_pv_event(&mut self, event: Event<PersistentVolume>) -> Result<()> {
        for volume in event.into_iter_applied() {
            if let PersistentVolume { spec: Some(PersistentVolumeSpec { storage_class_name: Some(storage_class_name), .. }), .. } = &volume {
                if storage_class_name != STORAGE_CLASS_NAME {
                    // Ignore any PVCs not assigned to our storage class
                    continue;
                }

                // Delete requested volumes
                if let ObjectMeta { deletion_timestamp: Some(_), finalizers: Some(finalizers), .. } = volume.metadata {
                    if !finalizers.iter().any(|finalizer| finalizer == FINALIZER_NAME) {
                        continue;
                    }

                    // TODO: Run helper pod to delete volume
                    // TODO: Remove finalizer in helper pod

                    continue;
                }

                if let Some(uid) = volume.uid() {
                    self.active_pv_uids.insert(uid);
                }
            }
        }

        Ok(())
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

    async fn ensure_storage_class_exists(&self) -> Result<()> {
        let client = self.client();

        let storage_classes = Api::<StorageClass>::all(client);

        storage_classes.entry("btrfs-provisioner")
            .await?
            .or_insert(|| {
                println!("Creating StorageClass");

                StorageClass {
                    provisioner: PROVISIONER_NAME.into(),
                    allow_volume_expansion: Some(false),
                    metadata: ObjectMeta {
                        name: Some(STORAGE_CLASS_NAME.into()),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .commit(&PostParams::default())
            .await?;

        Ok(())
    }

    async fn deploy_helper_pod(&self, name: &str, node_name: &str, args: &[&str]) -> Result<()> {
        let client = self.client();
        let pods = Api::<Pod>::namespaced(client, NAMESPACE);

        pods.create(&PostParams::default(), &Pod {
            metadata: ObjectMeta {
                generate_name: Some(name.to_owned() + "-"),
                ..ObjectMeta::default()
            },
            spec: Some(PodSpec {
                restart_policy: Some("OnFailure".into()),
                node_name: Some(node_name.into()),
                service_account_name: Some(SERVICE_ACCOUNT_NAME.into()),
                containers: vec![Container {
                    name: "provisioner".into(),
                    image: Some(IMAGE.into()),
                    image_pull_policy: Some("IfNotPresent".into()),
                    args: Some(args.iter().map(|s| String::from(*s)).collect()),
                    env: Some(vec![
                        EnvVar {
                            name: HOST_FS_ENV_NAME.into(),
                            value: Some("/host".into()),
                            ..EnvVar::default()
                        },
                        EnvVar {
                            name: "NODE_NAME".into(),
                            value_from: Some(EnvVarSource {
                                field_ref: Some(ObjectFieldSelector {
                                    field_path: "spec.nodeName".into(),
                                    ..ObjectFieldSelector::default()
                                }),
                                ..EnvVarSource::default()
                            }),
                            ..EnvVar::default()
                        },
                    ]),
                    security_context: Some(SecurityContext {
                        privileged: Some(true),
                        ..SecurityContext::default()
                    }),
                    volume_mounts: Some(vec![VolumeMount {
                        name: "host".into(),
                        mount_path: "/host".into(),
                        ..VolumeMount::default()
                    }]),
                    ..Container::default()
                }],
                volumes: Some(vec![Volume {
                    name: "host".into(),
                    host_path: Some(HostPathVolumeSource {
                        path: "/".into(),
                        ..HostPathVolumeSource::default()
                    }),
                    ..Volume::default()
                }]),
                ..PodSpec::default()
            }),
            ..Pod::default()
        }).await?;

        Ok(())
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
        let mut pathbuf = PathBuf::new();

        if let Ok(path) = std::env::var(HOST_FS_ENV_NAME) {
            pathbuf.push(path);
        }

        for part in path {
            pathbuf.push(part.trim_start_matches('/'));
        }

        Ok(pathbuf)
    }
}