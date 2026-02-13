use anyhow::{Context, Result};
use k8s_openapi::api::apps::v1::StatefulSet;
use k8s_openapi::api::core::v1::Pod;
use kube::api::{DeleteParams, Patch, PatchParams};
use kube::{Api, Client};
use serde_json::json;

use crate::types::ResourceType;

pub async fn delete_resource(
    client: Client,
    namespace: &str,
    name: &str,
    resource_type: ResourceType,
) -> Result<()> {
    match resource_type {
        ResourceType::Pods => {
            let api: Api<Pod> = Api::namespaced(client, namespace);
            api.delete(name, &DeleteParams::default())
                .await
                .context("Failed to delete pod")?;
        }
        ResourceType::PersistentVolumeClaims => {
            let api: Api<k8s_openapi::api::core::v1::PersistentVolumeClaim> =
                Api::namespaced(client, namespace);
            api.delete(name, &DeleteParams::default())
                .await
                .context("Failed to delete PVC")?;
        }
        ResourceType::StatefulSets => {
            let api: Api<StatefulSet> = Api::namespaced(client, namespace);
            api.delete(name, &DeleteParams::default())
                .await
                .context("Failed to delete StatefulSet")?;
        }
    }
    Ok(())
}

pub async fn restart_resource(
    client: Client,
    namespace: &str,
    name: &str,
    resource_type: ResourceType,
) -> Result<()> {
    match resource_type {
        ResourceType::Pods => {
            // Restart a pod by deleting it (controller will recreate)
            let api: Api<Pod> = Api::namespaced(client, namespace);
            api.delete(name, &DeleteParams::default())
                .await
                .context("Failed to restart pod (delete)")?;
        }
        ResourceType::StatefulSets => {
            // Rollout restart via annotation patch
            let api: Api<StatefulSet> = Api::namespaced(client, namespace);
            let now = {
                use std::time::SystemTime;
                let d = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default();
                // Simple ISO 8601 timestamp
                let secs = d.as_secs();
                format!("{}", secs)
            };
            let patch = json!({
                "spec": {
                    "template": {
                        "metadata": {
                            "annotations": {
                                "kubectl.kubernetes.io/restartedAt": now
                            }
                        }
                    }
                }
            });
            api.patch(name, &PatchParams::default(), &Patch::Merge(&patch))
                .await
                .context("Failed to restart StatefulSet")?;
        }
        ResourceType::PersistentVolumeClaims => {
            anyhow::bail!("PVCs cannot be restarted");
        }
    }
    Ok(())
}

pub async fn apply_yaml(
    client: Client,
    namespace: &str,
    name: &str,
    resource_type: ResourceType,
    yaml_str: &str,
) -> Result<()> {
    match resource_type {
        ResourceType::Pods => {
            let api: Api<Pod> = Api::namespaced(client, namespace);
            let data: Pod = serde_yaml::from_str(yaml_str).context("Invalid Pod YAML")?;
            api.replace(name, &kube::api::PostParams::default(), &data)
                .await
                .context("Failed to apply Pod YAML")?;
        }
        ResourceType::PersistentVolumeClaims => {
            let api: Api<k8s_openapi::api::core::v1::PersistentVolumeClaim> =
                Api::namespaced(client, namespace);
            let data: k8s_openapi::api::core::v1::PersistentVolumeClaim =
                serde_yaml::from_str(yaml_str).context("Invalid PVC YAML")?;
            api.replace(name, &kube::api::PostParams::default(), &data)
                .await
                .context("Failed to apply PVC YAML")?;
        }
        ResourceType::StatefulSets => {
            let api: Api<StatefulSet> = Api::namespaced(client, namespace);
            let data: StatefulSet =
                serde_yaml::from_str(yaml_str).context("Invalid StatefulSet YAML")?;
            api.replace(name, &kube::api::PostParams::default(), &data)
                .await
                .context("Failed to apply StatefulSet YAML")?;
        }
    }
    Ok(())
}
