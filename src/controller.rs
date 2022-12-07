use std::collections::HashSet;

use color_eyre::Result;
use futures_util::{stream, StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::{Container, EnvVar, EnvVarSource, HostPathVolumeSource, ObjectFieldSelector, PersistentVolume, PersistentVolumeClaim, PersistentVolumeClaimSpec, PersistentVolumeClaimStatus, PersistentVolumeSpec, Pod, PodSpec, SecurityContext, Volume, VolumeMount};
use k8s_openapi::api::storage::v1::StorageClass;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::{Api, Client, Config, ResourceExt};
use kube::api::{ListParams, PostParams};
use kube::runtime::{reflector, watcher};
use kube::runtime::watcher::Event;
use mkdirp::mkdirp;

use crate::config::*;
use crate::ext::ProvisionerResourceExt;

enum WatchedResource {
    Pv(Event<PersistentVolume>),
    Pvc(Event<PersistentVolumeClaim>),
}

pub struct Controller {
    client: Client,
    active_pvc_uids: HashSet<String>,
    active_pv_uids: HashSet<String>,
}

impl Controller {
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

    pub async fn run(&mut self) -> Result<()> {
        Controller::prepare_directories()?;

        self.ensure_storage_class_exists().await?;

        println!("Provisioner started.");

        self.watch_persistent_volume_claims().await?;

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
}