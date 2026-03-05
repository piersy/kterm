use anyhow::{Context, Result};
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::autoscaling::v1::HorizontalPodAutoscaler;
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{
    ConfigMap, Endpoints, Event, LimitRange, Namespace, Node, PersistentVolume,
    PersistentVolumeClaim, Pod, ReplicationController, ResourceQuota, Secret, Service,
    ServiceAccount,
};
use k8s_openapi::api::networking::v1::{Ingress, NetworkPolicy};
use k8s_openapi::api::policy::v1::PodDisruptionBudget;
use k8s_openapi::api::storage::v1::StorageClass;
use kube::api::{DeleteParams, Patch, PatchParams};
use kube::{Api, Client};
use serde::de::DeserializeOwned;
use serde::Serialize;
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
            delete_namespaced::<Pod>(client, namespace, name, "pod").await
        }
        ResourceType::Deployments => {
            delete_namespaced::<Deployment>(client, namespace, name, "Deployment").await
        }
        ResourceType::StatefulSets => {
            delete_namespaced::<StatefulSet>(client, namespace, name, "StatefulSet").await
        }
        ResourceType::DaemonSets => {
            delete_namespaced::<DaemonSet>(client, namespace, name, "DaemonSet").await
        }
        ResourceType::ReplicaSets => {
            delete_namespaced::<ReplicaSet>(client, namespace, name, "ReplicaSet").await
        }
        ResourceType::ReplicationControllers => {
            delete_namespaced::<ReplicationController>(client, namespace, name, "ReplicationController").await
        }
        ResourceType::Jobs => {
            delete_namespaced::<Job>(client, namespace, name, "Job").await
        }
        ResourceType::CronJobs => {
            delete_namespaced::<CronJob>(client, namespace, name, "CronJob").await
        }
        ResourceType::HorizontalPodAutoscalers => {
            delete_namespaced::<HorizontalPodAutoscaler>(client, namespace, name, "HPA").await
        }
        ResourceType::Services => {
            delete_namespaced::<Service>(client, namespace, name, "Service").await
        }
        ResourceType::Endpoints => {
            delete_namespaced::<Endpoints>(client, namespace, name, "Endpoints").await
        }
        ResourceType::Ingresses => {
            delete_namespaced::<Ingress>(client, namespace, name, "Ingress").await
        }
        ResourceType::NetworkPolicies => {
            delete_namespaced::<NetworkPolicy>(client, namespace, name, "NetworkPolicy").await
        }
        ResourceType::ConfigMaps => {
            delete_namespaced::<ConfigMap>(client, namespace, name, "ConfigMap").await
        }
        ResourceType::Secrets => {
            delete_namespaced::<Secret>(client, namespace, name, "Secret").await
        }
        ResourceType::PersistentVolumeClaims => {
            delete_namespaced::<PersistentVolumeClaim>(client, namespace, name, "PVC").await
        }
        ResourceType::ServiceAccounts => {
            delete_namespaced::<ServiceAccount>(client, namespace, name, "ServiceAccount").await
        }
        ResourceType::Events => {
            delete_namespaced::<Event>(client, namespace, name, "Event").await
        }
        ResourceType::ResourceQuotas => {
            delete_namespaced::<ResourceQuota>(client, namespace, name, "ResourceQuota").await
        }
        ResourceType::LimitRanges => {
            delete_namespaced::<LimitRange>(client, namespace, name, "LimitRange").await
        }
        ResourceType::PodDisruptionBudgets => {
            delete_namespaced::<PodDisruptionBudget>(client, namespace, name, "PDB").await
        }
        // Cluster-scoped
        ResourceType::PersistentVolumes => {
            delete_cluster::<PersistentVolume>(client, name, "PersistentVolume").await
        }
        ResourceType::StorageClasses => {
            delete_cluster::<StorageClass>(client, name, "StorageClass").await
        }
        ResourceType::Namespaces => {
            delete_cluster::<Namespace>(client, name, "Namespace").await
        }
        ResourceType::Nodes => {
            delete_cluster::<Node>(client, name, "Node").await
        }
    }
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
        ResourceType::Deployments => {
            rollout_restart::<Deployment>(client, namespace, name, "Deployment").await?;
        }
        ResourceType::StatefulSets => {
            rollout_restart::<StatefulSet>(client, namespace, name, "StatefulSet").await?;
        }
        ResourceType::DaemonSets => {
            rollout_restart::<DaemonSet>(client, namespace, name, "DaemonSet").await?;
        }
        _ => {
            anyhow::bail!("{} resources cannot be restarted", resource_type);
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
            apply_namespaced::<Pod>(client, namespace, name, yaml_str, "Pod").await
        }
        ResourceType::Deployments => {
            apply_namespaced::<Deployment>(client, namespace, name, yaml_str, "Deployment").await
        }
        ResourceType::StatefulSets => {
            apply_namespaced::<StatefulSet>(client, namespace, name, yaml_str, "StatefulSet").await
        }
        ResourceType::DaemonSets => {
            apply_namespaced::<DaemonSet>(client, namespace, name, yaml_str, "DaemonSet").await
        }
        ResourceType::ReplicaSets => {
            apply_namespaced::<ReplicaSet>(client, namespace, name, yaml_str, "ReplicaSet").await
        }
        ResourceType::ReplicationControllers => {
            apply_namespaced::<ReplicationController>(
                client, namespace, name, yaml_str, "ReplicationController",
            )
            .await
        }
        ResourceType::Jobs => {
            apply_namespaced::<Job>(client, namespace, name, yaml_str, "Job").await
        }
        ResourceType::CronJobs => {
            apply_namespaced::<CronJob>(client, namespace, name, yaml_str, "CronJob").await
        }
        ResourceType::HorizontalPodAutoscalers => {
            apply_namespaced::<HorizontalPodAutoscaler>(
                client, namespace, name, yaml_str, "HPA",
            )
            .await
        }
        ResourceType::Services => {
            apply_namespaced::<Service>(client, namespace, name, yaml_str, "Service").await
        }
        ResourceType::Endpoints => {
            apply_namespaced::<Endpoints>(client, namespace, name, yaml_str, "Endpoints").await
        }
        ResourceType::Ingresses => {
            apply_namespaced::<Ingress>(client, namespace, name, yaml_str, "Ingress").await
        }
        ResourceType::NetworkPolicies => {
            apply_namespaced::<NetworkPolicy>(client, namespace, name, yaml_str, "NetworkPolicy")
                .await
        }
        ResourceType::ConfigMaps => {
            apply_namespaced::<ConfigMap>(client, namespace, name, yaml_str, "ConfigMap").await
        }
        ResourceType::Secrets => {
            apply_namespaced::<Secret>(client, namespace, name, yaml_str, "Secret").await
        }
        ResourceType::PersistentVolumeClaims => {
            apply_namespaced::<PersistentVolumeClaim>(client, namespace, name, yaml_str, "PVC")
                .await
        }
        ResourceType::ServiceAccounts => {
            apply_namespaced::<ServiceAccount>(
                client, namespace, name, yaml_str, "ServiceAccount",
            )
            .await
        }
        ResourceType::Events => {
            apply_namespaced::<Event>(client, namespace, name, yaml_str, "Event").await
        }
        ResourceType::ResourceQuotas => {
            apply_namespaced::<ResourceQuota>(client, namespace, name, yaml_str, "ResourceQuota")
                .await
        }
        ResourceType::LimitRanges => {
            apply_namespaced::<LimitRange>(client, namespace, name, yaml_str, "LimitRange").await
        }
        ResourceType::PodDisruptionBudgets => {
            apply_namespaced::<PodDisruptionBudget>(client, namespace, name, yaml_str, "PDB").await
        }
        // Cluster-scoped
        ResourceType::PersistentVolumes => {
            apply_cluster::<PersistentVolume>(client, name, yaml_str, "PersistentVolume").await
        }
        ResourceType::StorageClasses => {
            apply_cluster::<StorageClass>(client, name, yaml_str, "StorageClass").await
        }
        ResourceType::Namespaces => {
            apply_cluster::<Namespace>(client, name, yaml_str, "Namespace").await
        }
        ResourceType::Nodes => {
            apply_cluster::<Node>(client, name, yaml_str, "Node").await
        }
    }
}

// ---------------------------------------------------------------------------
// Generic helpers
// ---------------------------------------------------------------------------

async fn delete_namespaced<T>(
    client: Client,
    namespace: &str,
    name: &str,
    label: &str,
) -> Result<()>
where
    T: kube::Resource<DynamicType = (), Scope = kube::core::NamespaceResourceScope>
        + Clone
        + DeserializeOwned
        + std::fmt::Debug
        + Send
        + Sync
        + 'static,
{
    let api: Api<T> = Api::namespaced(client, namespace);
    api.delete(name, &DeleteParams::default())
        .await
        .context(format!("Failed to delete {}", label))?;
    Ok(())
}

async fn delete_cluster<T>(client: Client, name: &str, label: &str) -> Result<()>
where
    T: kube::Resource<DynamicType = ()>
        + Clone
        + DeserializeOwned
        + std::fmt::Debug
        + Send
        + Sync
        + 'static,
{
    let api: Api<T> = Api::all(client);
    api.delete(name, &DeleteParams::default())
        .await
        .context(format!("Failed to delete {}", label))?;
    Ok(())
}

async fn rollout_restart<T>(
    client: Client,
    namespace: &str,
    name: &str,
    label: &str,
) -> Result<()>
where
    T: kube::Resource<DynamicType = (), Scope = kube::core::NamespaceResourceScope>
        + Clone
        + DeserializeOwned
        + Serialize
        + std::fmt::Debug
        + Send
        + Sync
        + 'static,
{
    let api: Api<T> = Api::namespaced(client, namespace);
    let now = {
        use std::time::SystemTime;
        let d = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        format!("{}", d.as_secs())
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
        .context(format!("Failed to restart {}", label))?;
    Ok(())
}

async fn apply_namespaced<T>(
    client: Client,
    namespace: &str,
    name: &str,
    yaml_str: &str,
    label: &str,
) -> Result<()>
where
    T: kube::Resource<DynamicType = (), Scope = kube::core::NamespaceResourceScope>
        + Clone
        + DeserializeOwned
        + Serialize
        + std::fmt::Debug
        + Send
        + Sync
        + 'static,
{
    let api: Api<T> = Api::namespaced(client, namespace);
    let data: T =
        serde_yaml::from_str(yaml_str).context(format!("Invalid {} YAML", label))?;
    api.replace(name, &kube::api::PostParams::default(), &data)
        .await
        .context(format!("Failed to apply {} YAML", label))?;
    Ok(())
}

async fn apply_cluster<T>(
    client: Client,
    name: &str,
    yaml_str: &str,
    label: &str,
) -> Result<()>
where
    T: kube::Resource<DynamicType = ()>
        + Clone
        + DeserializeOwned
        + Serialize
        + std::fmt::Debug
        + Send
        + Sync
        + 'static,
{
    let api: Api<T> = Api::all(client);
    let data: T =
        serde_yaml::from_str(yaml_str).context(format!("Invalid {} YAML", label))?;
    api.replace(name, &kube::api::PostParams::default(), &data)
        .await
        .context(format!("Failed to apply {} YAML", label))?;
    Ok(())
}
