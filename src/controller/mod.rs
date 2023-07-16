use std::collections::{BTreeMap, HashSet};
use color_eyre::eyre::{eyre};

use color_eyre::Result;
use futures_util::{stream, StreamExt, TryStreamExt};
use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::api::core::v1::{Container, EnvVar, EnvVarSource, HostPathVolumeSource, Node, ObjectFieldSelector, PersistentVolume, PersistentVolumeClaim, PersistentVolumeClaimSpec, PersistentVolumeClaimStatus, PersistentVolumeSpec, PodSpec, PodTemplateSpec, SecurityContext, Volume, VolumeMount};
use k8s_openapi::api::storage::v1::StorageClass;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::{Api, Client, Config, ResourceExt};
use kube::api::{ListParams, PostParams};
use kube::runtime::{reflector, watcher};
use kube::runtime::watcher::Event;

use crate::config::*;
use crate::controller::provisioner_job_type::{DeleteJobArgs, InitializeNodeJobArgs, ProvisionerJobType, ProvisionJobArgs};
use crate::controller::storage_class_utils::{get_node_assigned_to_storage_class, is_controlling_storage_class, StorageClassNodeAssignment};
use crate::ext::ProvisionerResourceExt;

pub mod provisioner_job_type;
pub mod storage_class_utils;

enum WatchedResource {
    Pv(Event<PersistentVolume>),
    Pvc(Event<PersistentVolumeClaim>),
    Node(Event<Node>),
}

enum RunJobResult {
    Deployed,
    AlreadyExisting(Job),
}

/// The [Controller] part watches cluster resources and reconciles any state
/// related to btrfs-provisioner. For example, it deploys Jobs to provision
/// new PVCs and delete PVs on demand.
pub struct Controller {
    /// The Kubernetes client to use, created in [Provisioner::create]
    client: Client,
    /// Collection of UIDs of all active PVCs managed by btrfs-provisioner
    active_pvc_uids: HashSet<String>,
    /// Collection of UIDs of all active PVs managed by btrfs-provisioner
    active_pv_uids: HashSet<String>,
}

impl Controller {
    /// Creates and returns a new [Controller].
    ///
    /// This method first tries to get the Kubernetes client credentials from ~/.kube/config and
    /// tries the in-cluster service account if it doesn't find any.
    pub async fn create() -> Result<Self> {
        let client = Client::try_default()
            .await
            .or_else(|_| Client::try_from(Config::incluster_env().expect("Failed to load in-cluster Kube config")))
            .expect("Failed to create Kube client");

        Ok(Controller {
            client,
            active_pvc_uids: HashSet::new(),
            active_pv_uids: HashSet::new(),
        })
    }

    /// Starts the Controller
    pub async fn run(&mut self) -> Result<()> {
        if *DYNAMIC_STORAGE_CLASS_ENABLED {
            todo!("Dynamic StorageClass is not supported yet (DYNAMIC_STORAGE_CLASS_ENABLED=true)");
        }

        println!("Controller started.");

        self.watch_resources().await?;

        Ok(())
    }

    /// Returns a copy of the Kubernetes client
    fn client(&self) -> Client {
        self.client.clone()
    }

    /// Watches related cluster resources and processes events
    ///
    /// This method only returns if an error occurs.
    async fn watch_resources(&mut self) -> Result<()> {
        let persistent_volume_claims = Api::<PersistentVolumeClaim>::all(self.client());
        let persistent_volumes = Api::<PersistentVolume>::all(self.client());
        let nodes = Api::<Node>::all(self.client());

        let (_, pvc_writer) = reflector::store();
        let (_, pv_writer) = reflector::store();
        let (_, node_writer) = reflector::store();
        let pvc_reflector = reflector(pvc_writer, watcher(persistent_volume_claims, watcher::Config::default()))
            .map_ok(WatchedResource::Pvc);
        let pv_reflector = reflector(pv_writer, watcher(persistent_volumes, watcher::Config::default()))
            .map_ok(WatchedResource::Pv);
        let node_reflector = reflector(node_writer, watcher(nodes, watcher::Config {
            label_selector: Some("!node-role.kubernetes.io/master".into()),
            ..watcher::Config::default()
        }))
            .map_ok(WatchedResource::Node);

        let stream = stream::select_all(vec![pvc_reflector.boxed(), pv_reflector.boxed(), node_reflector.boxed()]);

        tokio::pin!(stream);

        // Redirect the events to their respective event handlers, depending on
        // what resource the event is for
        while let Ok(Some(watched_resource)) = stream.try_next().await {
            match watched_resource {
                WatchedResource::Pvc(pvc) => self.process_pvc_event(pvc).await?,
                WatchedResource::Pv(pv) => self.process_pv_event(pv).await?,
                WatchedResource::Node(node) => self.process_node_event(node).await?,
            }
        };

        Ok(())
    }

    /// Process updates to PVCs
    async fn process_pvc_event(&mut self, event: Event<PersistentVolumeClaim>) -> Result<()> {
        for claim in event.into_iter_applied() {
            if let PersistentVolumeClaim { spec: Some(PersistentVolumeClaimSpec { storage_class_name: Some(storage_class_name), .. }), status: Some(PersistentVolumeClaimStatus { phase: Some(phase), .. }), .. } = &claim {
                // Ignore any PVCs not controlled by one of our storage classes
                if !is_controlling_storage_class(self.client(), storage_class_name).await? {
                    continue;
                }

                match phase.as_str() {
                    "Pending" => {
                        if let Some(uid) = &claim.uid() {
                            // We've seen this PVC before, skip.
                            if self.active_pvc_uids.contains(uid) {
                                continue;
                            }

                            println!("Pending: {}", &claim.full_name());
                            self.active_pvc_uids.insert(uid.clone());

                            let claim_namespace = &claim.namespace().unwrap();
                            let claim_name = &claim.name_any();

                            let assigned_node = get_node_assigned_to_storage_class(self.client(), storage_class_name)
                                .await?
                                .ok_or_else(|| eyre!("No node assigned with StorageClass"))?;

                            match assigned_node {
                                StorageClassNodeAssignment::SingleNode { node_name } => {
                                    println!("Deploying volume provisioning job on Node {}", node_name);
                                    if let Err(e) = self.run_provisioner_job("provision-volume", &node_name, &["provision", claim_namespace, claim_name], ProvisionerJobType::Provision(ProvisionJobArgs {
                                        target_pvc_uid: uid.to_owned(),
                                    })).await {
                                        eprintln!("{}", e);
                                    }
                                }
                                StorageClassNodeAssignment::Dynamic => {
                                    todo!("Dynamic StorageClass is not supported yet")
                                }
                            };
                        }
                    }
                    "Bound" => {
                        if let Some(uid) = &claim.uid() {
                            if self.active_pvc_uids.contains(uid) {
                                continue;
                            }

                            // This PVC is already bound so we have nothing to do
                            self.active_pvc_uids.insert(uid.clone());
                            println!("Bound: {}", &claim.full_name());
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Process updates to PVs
    async fn process_pv_event(&mut self, event: Event<PersistentVolume>) -> Result<()> {
        for volume in event.into_iter_applied() {
            if let PersistentVolume {
                metadata: ObjectMeta {
                    uid: Some(uid), ..
                },
                spec: Some(
                    PersistentVolumeSpec {
                        storage_class_name: Some(storage_class_name), ..
                    }
                ), ..
            } = &volume {
                // Ignore any PVs not controlled by one of our storage classes
                if !is_controlling_storage_class(self.client(), storage_class_name).await? {
                    continue;
                }

                // Delete requested volumes
                if let PersistentVolume {
                    metadata: ObjectMeta {
                        deletion_timestamp: Some(_),
                        finalizers: Some(ref finalizers), ..
                    }, ..
                } = volume {
                    // Skip volume if it doesn't have our finalizer anymore
                    if !finalizers.iter().any(|f| f == FINALIZER_NAME) {
                        continue;
                    }

                    match Controller::get_node_hostname_from_node_affinity(&volume) {
                        Some(node_hostname) => {
                            let nodes = Api::<Node>::all(self.client());

                            // Find the node name from the node hostname
                            let volume_nodes = nodes.list(&ListParams {
                                label_selector: Some(format!("{}={}", NODE_HOSTNAME_KEY, node_hostname)),
                                limit: Some(1),
                                ..ListParams::default()
                            }).await?;

                            if let Some(node_name) = &volume_nodes.items.get(0).and_then(|i| i.metadata.name.as_ref()) {
                                println!("Deploying volume deletion job on Node {}", node_name);
                                if let Err(e) = self.run_provisioner_job("delete-volume", node_name, &["delete", volume.name_any().as_str()], ProvisionerJobType::Delete(DeleteJobArgs {
                                    target_pv_uid: uid.to_owned(),
                                })).await {
                                    eprintln!("{}", e);
                                }
                            } else {
                                eprintln!("Did not find node with {}={}", NODE_HOSTNAME_KEY, node_hostname)
                            }

                            continue;
                        }
                        None => {
                            eprintln!("PV {} should be deleted but does not have NodeAffinity set, don't know what Node to schedule the helper job on", volume.name_any())
                        }
                    }
                }

                if let Some(uid) = volume.uid() {
                    self.active_pv_uids.insert(uid);
                }
            }
        }

        Ok(())
    }

    /// Process updates to Nodes
    async fn process_node_event(&self, event: Event<Node>) -> Result<()> {
        for node in event.into_iter_applied() {
            if let Some(uid) = &node.metadata.uid {
                let storage_classes = Api::<StorageClass>::all(self.client());

                if let [existing_storage_class] = storage_classes.list(&ListParams {
                    label_selector: Some(format!("{}={}", STORAGE_CLASS_CONTROLLING_NODE_LABEL_NAME, &node.name_any())),
                    limit: Some(1),
                    ..ListParams::default()
                }).await?.items.as_slice() {
                    println!("Node {} is associated with StorageClass {}", node.name_any(), existing_storage_class.name_any());
                    continue;
                }

                println!("Initializing Node {}", node.name_any());
                self.run_provisioner_job("initialize-node", &node.name_any(), &["initialize-node"], ProvisionerJobType::InitializeNode(InitializeNodeJobArgs {
                    target_node_uid: uid.to_owned(),
                })).await?;
            }
        }

        Ok(())
    }

    /// Tries to extract the Node hostname from a [PersistentVolume] by looking at the `nodeAffinity` field.
    fn get_node_hostname_from_node_affinity(volume: &PersistentVolume) -> Option<String> {
        volume
            .spec.as_ref()?
            .node_affinity.as_ref()?
            .required.as_ref()?
            .node_selector_terms.get(0)?
            .match_expressions.as_ref()?
            .iter()
            .filter(|r| r.key == NODE_HOSTNAME_KEY && r.operator == "In")
            .find_map(|r| r.values.as_ref()?.get(0).cloned())
    }

    /// Makes sure the StorageClass named [DYNAMIC_STORAGE_CLASS_NAME] exists in the cluster
    #[allow(dead_code, unreachable_code)]
    async fn ensure_dynamic_storage_class_exists(&self) -> Result<()> {
        todo!("Dynamic StorageClasses are not supported yet");

        let storage_classes = Api::<StorageClass>::all(self.client());

        storage_classes.entry(&DYNAMIC_STORAGE_CLASS_NAME)
            .await?
            .or_insert(|| {
                println!("Creating dynamic StorageClass {}", *DYNAMIC_STORAGE_CLASS_NAME);

                StorageClass {
                    provisioner: PROVISIONER_NAME.into(),
                    allow_volume_expansion: Some(false),
                    metadata: ObjectMeta {
                        name: Some(STORAGE_CLASS_PER_NODE_NAME_PATTERN.to_owned()),
                        labels: Some(BTreeMap::from([
                            (STORAGE_CLASS_CONTROLLING_NODE_LABEL_NAME.into(), "*".into())
                        ])),
                        ..ObjectMeta::default()
                    },
                    ..StorageClass::default()
                }
            })
            .commit(&PostParams::default())
            .await?;

        Ok(())
    }

    /// Runs a [Provisioner] job as a Kubernetes Job.
    ///
    /// # Arguments
    ///
    /// - `name` - The name of the Job. Will have random characters appended.
    /// - `node_name` - The name of the Node the Job should schedule its Pod on
    /// - `args` - CLI arguments for the btrfs-provisioner binary
    /// - `job_type` - A [JobType] to use for finding existing Jobs
    async fn run_provisioner_job(&self, name: &str, node_name: &str, args: &[&str], job_type: ProvisionerJobType) -> Result<RunJobResult> {
        let jobs = Api::<Job>::namespaced(self.client(), NAMESPACE.as_str());

        // Cancel if there already is a job matching job_type's labels
        if let [existing_lob] = jobs.list(&ListParams {
            label_selector: Some(job_type.to_label_selector()),
            limit: Some(1),
            ..ListParams::default()
        }).await?.items.as_slice() {
            return Ok(RunJobResult::AlreadyExisting(existing_lob.to_owned()));
        }

        // Deploy the Job...
        jobs.create(&PostParams::default(), &Job {
            metadata: ObjectMeta {
                generate_name: Some(name.to_owned() + "-"),
                labels: Some(job_type.to_labels()),
                ..ObjectMeta::default()
            },
            spec: Some(JobSpec {
                ttl_seconds_after_finished: Some(600),
                template: PodTemplateSpec {
                    spec: Some(PodSpec {
                        restart_policy: Some("OnFailure".into()),
                        node_name: Some(node_name.into()),
                        service_account_name: Some(SERVICE_ACCOUNT_NAME.into()),
                        containers: vec![Container {
                            name: "provisioner".into(),
                            image: Some(IMAGE.to_owned()),
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
                                EnvVar {
                                    name: "VOLUMES_DIR".into(),
                                    value: Some(VOLUMES_DIR.to_owned()),
                                    ..EnvVar::default()
                                },
                                EnvVar {
                                    name: "ARCHIVE_ON_DELETE".into(),
                                    value: Some(if *ARCHIVE_ON_DELETE { "true" } else { "false" }.into()),
                                    ..EnvVar::default()
                                },
                                EnvVar {
                                    name: "STORAGE_CLASS_PER_NODE_ENABLED".into(),
                                    value: Some(if *STORAGE_CLASS_PER_NODE_ENABLED { "true" } else { "false" }.into()),
                                    ..EnvVar::default()
                                },
                                EnvVar {
                                    name: "STORAGE_CLASS_PER_NODE_NAME_PATTERN".into(),
                                    value: Some(STORAGE_CLASS_PER_NODE_NAME_PATTERN.to_owned()),
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
                    ..PodTemplateSpec::default()
                },
                ..JobSpec::default()
            }),
            ..Job::default()
        }).await?;

        Ok(RunJobResult::Deployed)
    }
}