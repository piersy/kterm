use std::collections::BTreeMap;
use std::fmt::Debug;

use anyhow::Result;
use futures::{StreamExt, TryStreamExt};
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
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kube::api::ListParams;
use kube::runtime::watcher;
use kube::runtime::WatchStreamExt;
use kube::{Api, Client, Resource, ResourceExt};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::mpsc;

use crate::event::AppEvent;
use crate::types::{ResourceItem, ResourceType};

// ---------------------------------------------------------------------------
// Generic watch / list / describe helpers
// ---------------------------------------------------------------------------

async fn watch_generic<T, F>(
    api: Api<T>,
    tx: mpsc::UnboundedSender<AppEvent>,
    converter: F,
) -> Result<()>
where
    T: Resource<DynamicType = ()> + Clone + DeserializeOwned + Debug + Send + Sync + 'static,
    F: Fn(&T) -> ResourceItem,
{
    let mut stream = watcher(api, watcher::Config::default())
        .default_backoff()
        .boxed();

    let mut cache: BTreeMap<String, T> = BTreeMap::new();

    while let Some(event) = stream.try_next().await? {
        match event {
            watcher::Event::Apply(obj) | watcher::Event::InitApply(obj) => {
                let name = ResourceExt::name_any(&obj);
                let ns = ResourceExt::namespace(&obj).unwrap_or_default();
                let key = format!("{}/{}", ns, name);
                cache.insert(key, obj);
            }
            watcher::Event::Delete(obj) => {
                let name = ResourceExt::name_any(&obj);
                let ns = ResourceExt::namespace(&obj).unwrap_or_default();
                let key = format!("{}/{}", ns, name);
                cache.remove(&key);
            }
            watcher::Event::Init => {
                cache.clear();
            }
            watcher::Event::InitDone => {}
        }

        let items: Vec<ResourceItem> = cache.values().map(&converter).collect();
        if tx.send(AppEvent::ResourcesUpdated(items)).is_err() {
            break;
        }
    }

    Ok(())
}

async fn list_generic<T, F>(api: Api<T>, converter: F) -> Result<Vec<ResourceItem>>
where
    T: Resource<DynamicType = ()> + Clone + DeserializeOwned + Debug + Send + Sync + 'static,
    F: Fn(&T) -> ResourceItem,
{
    let list = api.list(&ListParams::default()).await?;
    Ok(list.items.iter().map(converter).collect())
}

async fn describe_generic<T>(api: Api<T>, name: &str) -> Result<String>
where
    T: Resource<DynamicType = ()> + Clone + DeserializeOwned + Debug + Serialize + Send + Sync + 'static,
{
    let obj = api.get(name).await?;
    let mut desc = String::new();
    desc.push_str("\n--- Full YAML ---\n");
    if let Ok(yaml) = serde_yaml::to_string(&obj) {
        desc.push_str(&yaml);
    }
    Ok(desc)
}

// ---------------------------------------------------------------------------
// Public dispatch functions
// ---------------------------------------------------------------------------

pub async fn watch_resources(
    client: Client,
    namespace: &str,
    resource_type: ResourceType,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    match resource_type {
        ResourceType::Pods => {
            watch_generic(Api::<Pod>::namespaced(client, namespace), tx, pod_to_resource_item).await
        }
        ResourceType::Deployments => {
            watch_generic(
                Api::<Deployment>::namespaced(client, namespace),
                tx,
                deployment_to_resource_item,
            )
            .await
        }
        ResourceType::StatefulSets => {
            watch_generic(
                Api::<StatefulSet>::namespaced(client, namespace),
                tx,
                statefulset_to_resource_item,
            )
            .await
        }
        ResourceType::DaemonSets => {
            watch_generic(
                Api::<DaemonSet>::namespaced(client, namespace),
                tx,
                daemonset_to_resource_item,
            )
            .await
        }
        ResourceType::ReplicaSets => {
            watch_generic(
                Api::<ReplicaSet>::namespaced(client, namespace),
                tx,
                replicaset_to_resource_item,
            )
            .await
        }
        ResourceType::ReplicationControllers => {
            watch_generic(
                Api::<ReplicationController>::namespaced(client, namespace),
                tx,
                replication_controller_to_resource_item,
            )
            .await
        }
        ResourceType::Jobs => {
            watch_generic(Api::<Job>::namespaced(client, namespace), tx, job_to_resource_item).await
        }
        ResourceType::CronJobs => {
            watch_generic(
                Api::<CronJob>::namespaced(client, namespace),
                tx,
                cronjob_to_resource_item,
            )
            .await
        }
        ResourceType::HorizontalPodAutoscalers => {
            watch_generic(
                Api::<HorizontalPodAutoscaler>::namespaced(client, namespace),
                tx,
                hpa_to_resource_item,
            )
            .await
        }
        ResourceType::Services => {
            watch_generic(
                Api::<Service>::namespaced(client, namespace),
                tx,
                service_to_resource_item,
            )
            .await
        }
        ResourceType::Endpoints => {
            watch_generic(
                Api::<Endpoints>::namespaced(client, namespace),
                tx,
                endpoints_to_resource_item,
            )
            .await
        }
        ResourceType::Ingresses => {
            watch_generic(
                Api::<Ingress>::namespaced(client, namespace),
                tx,
                ingress_to_resource_item,
            )
            .await
        }
        ResourceType::NetworkPolicies => {
            watch_generic(
                Api::<NetworkPolicy>::namespaced(client, namespace),
                tx,
                network_policy_to_resource_item,
            )
            .await
        }
        ResourceType::ConfigMaps => {
            watch_generic(
                Api::<ConfigMap>::namespaced(client, namespace),
                tx,
                configmap_to_resource_item,
            )
            .await
        }
        ResourceType::Secrets => {
            watch_generic(
                Api::<Secret>::namespaced(client, namespace),
                tx,
                secret_to_resource_item,
            )
            .await
        }
        ResourceType::PersistentVolumeClaims => {
            watch_generic(
                Api::<PersistentVolumeClaim>::namespaced(client, namespace),
                tx,
                pvc_to_resource_item,
            )
            .await
        }
        ResourceType::PersistentVolumes => {
            watch_generic(Api::<PersistentVolume>::all(client), tx, pv_to_resource_item).await
        }
        ResourceType::StorageClasses => {
            watch_generic(
                Api::<StorageClass>::all(client),
                tx,
                storageclass_to_resource_item,
            )
            .await
        }
        ResourceType::ServiceAccounts => {
            watch_generic(
                Api::<ServiceAccount>::namespaced(client, namespace),
                tx,
                serviceaccount_to_resource_item,
            )
            .await
        }
        ResourceType::Namespaces => {
            watch_generic(Api::<Namespace>::all(client), tx, namespace_to_resource_item).await
        }
        ResourceType::Nodes => {
            watch_generic(Api::<Node>::all(client), tx, node_to_resource_item).await
        }
        ResourceType::Events => {
            watch_generic(
                Api::<Event>::namespaced(client, namespace),
                tx,
                event_to_resource_item,
            )
            .await
        }
        ResourceType::ResourceQuotas => {
            watch_generic(
                Api::<ResourceQuota>::namespaced(client, namespace),
                tx,
                resourcequota_to_resource_item,
            )
            .await
        }
        ResourceType::LimitRanges => {
            watch_generic(
                Api::<LimitRange>::namespaced(client, namespace),
                tx,
                limitrange_to_resource_item,
            )
            .await
        }
        ResourceType::PodDisruptionBudgets => {
            watch_generic(
                Api::<PodDisruptionBudget>::namespaced(client, namespace),
                tx,
                pdb_to_resource_item,
            )
            .await
        }
    }
}

pub async fn list_all_resources(
    client: Client,
    resource_type: ResourceType,
) -> Result<Vec<ResourceItem>> {
    match resource_type {
        ResourceType::Pods => list_generic(Api::<Pod>::all(client), pod_to_resource_item).await,
        ResourceType::Deployments => {
            list_generic(Api::<Deployment>::all(client), deployment_to_resource_item).await
        }
        ResourceType::StatefulSets => {
            list_generic(Api::<StatefulSet>::all(client), statefulset_to_resource_item).await
        }
        ResourceType::DaemonSets => {
            list_generic(Api::<DaemonSet>::all(client), daemonset_to_resource_item).await
        }
        ResourceType::ReplicaSets => {
            list_generic(Api::<ReplicaSet>::all(client), replicaset_to_resource_item).await
        }
        ResourceType::ReplicationControllers => {
            list_generic(
                Api::<ReplicationController>::all(client),
                replication_controller_to_resource_item,
            )
            .await
        }
        ResourceType::Jobs => list_generic(Api::<Job>::all(client), job_to_resource_item).await,
        ResourceType::CronJobs => {
            list_generic(Api::<CronJob>::all(client), cronjob_to_resource_item).await
        }
        ResourceType::HorizontalPodAutoscalers => {
            list_generic(
                Api::<HorizontalPodAutoscaler>::all(client),
                hpa_to_resource_item,
            )
            .await
        }
        ResourceType::Services => {
            list_generic(Api::<Service>::all(client), service_to_resource_item).await
        }
        ResourceType::Endpoints => {
            list_generic(Api::<Endpoints>::all(client), endpoints_to_resource_item).await
        }
        ResourceType::Ingresses => {
            list_generic(Api::<Ingress>::all(client), ingress_to_resource_item).await
        }
        ResourceType::NetworkPolicies => {
            list_generic(
                Api::<NetworkPolicy>::all(client),
                network_policy_to_resource_item,
            )
            .await
        }
        ResourceType::ConfigMaps => {
            list_generic(Api::<ConfigMap>::all(client), configmap_to_resource_item).await
        }
        ResourceType::Secrets => {
            list_generic(Api::<Secret>::all(client), secret_to_resource_item).await
        }
        ResourceType::PersistentVolumeClaims => {
            list_generic(
                Api::<PersistentVolumeClaim>::all(client),
                pvc_to_resource_item,
            )
            .await
        }
        ResourceType::PersistentVolumes => {
            list_generic(Api::<PersistentVolume>::all(client), pv_to_resource_item).await
        }
        ResourceType::StorageClasses => {
            list_generic(
                Api::<StorageClass>::all(client),
                storageclass_to_resource_item,
            )
            .await
        }
        ResourceType::ServiceAccounts => {
            list_generic(
                Api::<ServiceAccount>::all(client),
                serviceaccount_to_resource_item,
            )
            .await
        }
        ResourceType::Namespaces => {
            list_generic(Api::<Namespace>::all(client), namespace_to_resource_item).await
        }
        ResourceType::Nodes => {
            list_generic(Api::<Node>::all(client), node_to_resource_item).await
        }
        ResourceType::Events => {
            list_generic(Api::<Event>::all(client), event_to_resource_item).await
        }
        ResourceType::ResourceQuotas => {
            list_generic(
                Api::<ResourceQuota>::all(client),
                resourcequota_to_resource_item,
            )
            .await
        }
        ResourceType::LimitRanges => {
            list_generic(Api::<LimitRange>::all(client), limitrange_to_resource_item).await
        }
        ResourceType::PodDisruptionBudgets => {
            list_generic(
                Api::<PodDisruptionBudget>::all(client),
                pdb_to_resource_item,
            )
            .await
        }
    }
}

pub async fn describe_resource(
    client: Client,
    namespace: &str,
    name: &str,
    resource_type: ResourceType,
) -> Result<String> {
    match resource_type {
        // Specialized describe for common types
        ResourceType::Pods => describe_pod(client, namespace, name).await,
        ResourceType::PersistentVolumeClaims => describe_pvc(client, namespace, name).await,
        ResourceType::StatefulSets => describe_statefulset(client, namespace, name).await,
        // Generic describe (YAML) for the rest - namespaced
        ResourceType::Deployments => {
            describe_generic(Api::<Deployment>::namespaced(client, namespace), name).await
        }
        ResourceType::DaemonSets => {
            describe_generic(Api::<DaemonSet>::namespaced(client, namespace), name).await
        }
        ResourceType::ReplicaSets => {
            describe_generic(Api::<ReplicaSet>::namespaced(client, namespace), name).await
        }
        ResourceType::ReplicationControllers => {
            describe_generic(
                Api::<ReplicationController>::namespaced(client, namespace),
                name,
            )
            .await
        }
        ResourceType::Jobs => {
            describe_generic(Api::<Job>::namespaced(client, namespace), name).await
        }
        ResourceType::CronJobs => {
            describe_generic(Api::<CronJob>::namespaced(client, namespace), name).await
        }
        ResourceType::HorizontalPodAutoscalers => {
            describe_generic(
                Api::<HorizontalPodAutoscaler>::namespaced(client, namespace),
                name,
            )
            .await
        }
        ResourceType::Services => {
            describe_generic(Api::<Service>::namespaced(client, namespace), name).await
        }
        ResourceType::Endpoints => {
            describe_generic(Api::<Endpoints>::namespaced(client, namespace), name).await
        }
        ResourceType::Ingresses => {
            describe_generic(Api::<Ingress>::namespaced(client, namespace), name).await
        }
        ResourceType::NetworkPolicies => {
            describe_generic(Api::<NetworkPolicy>::namespaced(client, namespace), name).await
        }
        ResourceType::ConfigMaps => {
            describe_generic(Api::<ConfigMap>::namespaced(client, namespace), name).await
        }
        ResourceType::Secrets => {
            describe_generic(Api::<Secret>::namespaced(client, namespace), name).await
        }
        ResourceType::ServiceAccounts => {
            describe_generic(Api::<ServiceAccount>::namespaced(client, namespace), name).await
        }
        ResourceType::Events => {
            describe_generic(Api::<Event>::namespaced(client, namespace), name).await
        }
        ResourceType::ResourceQuotas => {
            describe_generic(Api::<ResourceQuota>::namespaced(client, namespace), name).await
        }
        ResourceType::LimitRanges => {
            describe_generic(Api::<LimitRange>::namespaced(client, namespace), name).await
        }
        ResourceType::PodDisruptionBudgets => {
            describe_generic(
                Api::<PodDisruptionBudget>::namespaced(client, namespace),
                name,
            )
            .await
        }
        // Cluster-scoped
        ResourceType::PersistentVolumes => {
            describe_generic(Api::<PersistentVolume>::all(client), name).await
        }
        ResourceType::StorageClasses => {
            describe_generic(Api::<StorageClass>::all(client), name).await
        }
        ResourceType::Namespaces => {
            describe_generic(Api::<Namespace>::all(client), name).await
        }
        ResourceType::Nodes => describe_generic(Api::<Node>::all(client), name).await,
    }
}

// ---------------------------------------------------------------------------
// Specialized describe functions (kept for rich output)
// ---------------------------------------------------------------------------

async fn describe_pod(client: Client, namespace: &str, name: &str) -> Result<String> {
    let api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let pod = api.get(name).await?;

    let mut desc = String::new();
    desc.push_str(&format!("Name:         {}\n", name));
    desc.push_str(&format!("Namespace:    {}\n", namespace));

    if let Some(ref status) = pod.status {
        let phase = status.phase.as_deref().unwrap_or("Unknown");
        desc.push_str(&format!("Status:       {}\n", phase));

        if let Some(ref pod_ip) = status.pod_ip {
            desc.push_str(&format!("IP:           {}\n", pod_ip));
        }

        if let Some(ref conditions) = status.conditions {
            desc.push_str("\nConditions:\n");
            for cond in conditions {
                desc.push_str(&format!(
                    "  {}: {} ({})\n",
                    cond.type_,
                    cond.status,
                    cond.reason.as_deref().unwrap_or("")
                ));
            }
        }

        if let Some(ref container_statuses) = status.container_statuses {
            desc.push_str("\nContainers:\n");
            for cs in container_statuses {
                desc.push_str(&format!("  {}:\n", cs.name));
                desc.push_str(&format!("    Image:    {}\n", cs.image));
                desc.push_str(&format!("    Ready:    {}\n", cs.ready));
                desc.push_str(&format!("    Restarts: {}\n", cs.restart_count));
            }
        }
    }

    if let Some(ref spec) = pod.spec {
        if let Some(ref node_name) = spec.node_name {
            desc.push_str(&format!("\nNode:         {}\n", node_name));
        }
    }

    let events = fetch_events(client, namespace, name).await;
    if !events.is_empty() {
        desc.push_str("\nEvents:\n");
        for event in &events {
            desc.push_str(&format!("  {}\n", event));
        }
    }

    desc.push_str("\n--- Full YAML ---\n");
    if let Ok(yaml) = serde_yaml::to_string(&pod) {
        desc.push_str(&yaml);
    }

    Ok(desc)
}

async fn describe_pvc(client: Client, namespace: &str, name: &str) -> Result<String> {
    let api: Api<PersistentVolumeClaim> = Api::namespaced(client.clone(), namespace);
    let pvc = api.get(name).await?;

    let mut desc = String::new();
    desc.push_str(&format!("Name:         {}\n", name));
    desc.push_str(&format!("Namespace:    {}\n", namespace));

    if let Some(ref status) = pvc.status {
        let phase = status.phase.as_deref().unwrap_or("Unknown");
        desc.push_str(&format!("Status:       {}\n", phase));

        if let Some(ref capacity) = status.capacity {
            if let Some(storage) = capacity.get("storage") {
                desc.push_str(&format!("Capacity:     {}\n", storage.0));
            }
        }
    }

    if let Some(ref spec) = pvc.spec {
        if let Some(ref volume_name) = spec.volume_name {
            desc.push_str(&format!("Volume:       {}\n", volume_name));
        }
        if let Some(ref sc) = spec.storage_class_name {
            desc.push_str(&format!("StorageClass: {}\n", sc));
        }
        if let Some(ref access_modes) = spec.access_modes {
            desc.push_str(&format!("AccessModes:  {:?}\n", access_modes));
        }
    }

    let events = fetch_events(client, namespace, name).await;
    if !events.is_empty() {
        desc.push_str("\nEvents:\n");
        for event in &events {
            desc.push_str(&format!("  {}\n", event));
        }
    }

    desc.push_str("\n--- Full YAML ---\n");
    if let Ok(yaml) = serde_yaml::to_string(&pvc) {
        desc.push_str(&yaml);
    }

    Ok(desc)
}

async fn describe_statefulset(client: Client, namespace: &str, name: &str) -> Result<String> {
    let api: Api<StatefulSet> = Api::namespaced(client.clone(), namespace);
    let ss = api.get(name).await?;

    let mut desc = String::new();
    desc.push_str(&format!("Name:         {}\n", name));
    desc.push_str(&format!("Namespace:    {}\n", namespace));

    if let Some(ref status) = ss.status {
        desc.push_str(&format!("Replicas:     {}\n", status.replicas));
        desc.push_str(&format!(
            "Ready:        {}\n",
            status.ready_replicas.unwrap_or(0)
        ));
        desc.push_str(&format!(
            "Updated:      {}\n",
            status.updated_replicas.unwrap_or(0)
        ));
    }

    if let Some(ref spec) = ss.spec {
        if let Some(replicas) = spec.replicas {
            desc.push_str(&format!("Desired:      {}\n", replicas));
        }
        desc.push_str(&format!(
            "ServiceName:  {}\n",
            spec.service_name.as_deref().unwrap_or("<none>")
        ));
    }

    let events = fetch_events(client, namespace, name).await;
    if !events.is_empty() {
        desc.push_str("\nEvents:\n");
        for event in &events {
            desc.push_str(&format!("  {}\n", event));
        }
    }

    desc.push_str("\n--- Full YAML ---\n");
    if let Ok(yaml) = serde_yaml::to_string(&ss) {
        desc.push_str(&yaml);
    }

    Ok(desc)
}

async fn fetch_events(client: Client, namespace: &str, resource_name: &str) -> Vec<String> {
    let events_api: Api<Event> = Api::namespaced(client, namespace);
    let lp = ListParams::default().fields(&format!("involvedObject.name={}", resource_name));

    match events_api.list(&lp).await {
        Ok(event_list) => event_list
            .items
            .iter()
            .map(|e| {
                let type_ = e.type_.as_deref().unwrap_or("Normal");
                let reason = e.reason.as_deref().unwrap_or("");
                let message = e.message.as_deref().unwrap_or("");
                format!("{} {} {}", type_, reason, message)
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Converter functions
// ---------------------------------------------------------------------------

fn pod_to_resource_item(pod: &Pod) -> ResourceItem {
    let name = ResourceExt::name_any(pod);
    let namespace = ResourceExt::namespace(pod).unwrap_or_default();

    let (status, restarts, node) = if let Some(ref s) = pod.status {
        let phase = s.phase.clone().unwrap_or_else(|| "Unknown".to_string());

        let status = s
            .container_statuses
            .as_ref()
            .and_then(|cs| {
                cs.iter().find_map(|c| {
                    c.state.as_ref().and_then(|state| {
                        if let Some(ref w) = state.waiting {
                            Some(w.reason.clone().unwrap_or_else(|| "Waiting".to_string()))
                        } else if state.terminated.is_some() {
                            Some("Terminated".to_string())
                        } else {
                            None
                        }
                    })
                })
            })
            .unwrap_or(phase);

        let restart_count: i32 = s
            .container_statuses
            .as_ref()
            .map(|cs| cs.iter().map(|c| c.restart_count).sum())
            .unwrap_or(0);

        let node_name = pod
            .spec
            .as_ref()
            .and_then(|spec| spec.node_name.clone())
            .unwrap_or_else(|| "<none>".to_string());

        (status, restart_count.to_string(), node_name)
    } else {
        ("Unknown".to_string(), "0".to_string(), "<none>".to_string())
    };

    let age = format_age(pod.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(pod).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status,
        age,
        extra: vec![
            ("restarts".to_string(), restarts),
            ("node".to_string(), node),
        ],
        raw_yaml,
    }
}

fn deployment_to_resource_item(deploy: &Deployment) -> ResourceItem {
    let name = ResourceExt::name_any(deploy);
    let namespace = ResourceExt::namespace(deploy).unwrap_or_default();

    let (status, ready, up_to_date, available) = if let Some(ref s) = deploy.status {
        let desired = deploy
            .spec
            .as_ref()
            .and_then(|spec| spec.replicas)
            .unwrap_or(0);
        let ready_count = s.ready_replicas.unwrap_or(0);
        let ready_str = format!("{}/{}", ready_count, desired);
        let up_to_date = s.updated_replicas.unwrap_or(0).to_string();
        let available = s.available_replicas.unwrap_or(0).to_string();
        let status = if ready_count == desired {
            "Active".to_string()
        } else {
            "Updating".to_string()
        };
        (status, ready_str, up_to_date, available)
    } else {
        (
            "Unknown".to_string(),
            "0/0".to_string(),
            "0".to_string(),
            "0".to_string(),
        )
    };

    let age = format_age(deploy.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(deploy).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status,
        age,
        extra: vec![
            ("ready".to_string(), ready),
            ("up-to-date".to_string(), up_to_date),
            ("available".to_string(), available),
        ],
        raw_yaml,
    }
}

fn statefulset_to_resource_item(ss: &StatefulSet) -> ResourceItem {
    let name = ResourceExt::name_any(ss);
    let namespace = ResourceExt::namespace(ss).unwrap_or_default();

    let (status, ready) = if let Some(ref s) = ss.status {
        let desired = ss
            .spec
            .as_ref()
            .and_then(|spec| spec.replicas)
            .unwrap_or(0);
        let ready_count = s.ready_replicas.unwrap_or(0);
        let ready_str = format!("{}/{}", ready_count, desired);
        let status = if ready_count == desired {
            "Active".to_string()
        } else {
            "Updating".to_string()
        };
        (status, ready_str)
    } else {
        ("Unknown".to_string(), "0/0".to_string())
    };

    let age = format_age(ss.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(ss).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status,
        age,
        extra: vec![("ready".to_string(), ready)],
        raw_yaml,
    }
}

fn daemonset_to_resource_item(ds: &DaemonSet) -> ResourceItem {
    let name = ResourceExt::name_any(ds);
    let namespace = ResourceExt::namespace(ds).unwrap_or_default();

    let (desired, current, ready) = if let Some(ref s) = ds.status {
        (
            s.desired_number_scheduled.to_string(),
            s.current_number_scheduled.to_string(),
            s.number_ready.to_string(),
        )
    } else {
        ("0".to_string(), "0".to_string(), "0".to_string())
    };

    let age = format_age(ds.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(ds).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![
            ("desired".to_string(), desired),
            ("current".to_string(), current),
            ("ready".to_string(), ready),
        ],
        raw_yaml,
    }
}

fn replicaset_to_resource_item(rs: &ReplicaSet) -> ResourceItem {
    let name = ResourceExt::name_any(rs);
    let namespace = ResourceExt::namespace(rs).unwrap_or_default();

    let (desired, current, ready) = if let Some(ref s) = rs.status {
        let desired = rs
            .spec
            .as_ref()
            .and_then(|spec| spec.replicas)
            .unwrap_or(0);
        (
            desired.to_string(),
            s.replicas.to_string(),
            s.ready_replicas.unwrap_or(0).to_string(),
        )
    } else {
        ("0".to_string(), "0".to_string(), "0".to_string())
    };

    let age = format_age(rs.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(rs).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![
            ("desired".to_string(), desired),
            ("current".to_string(), current),
            ("ready".to_string(), ready),
        ],
        raw_yaml,
    }
}

fn replication_controller_to_resource_item(rc: &ReplicationController) -> ResourceItem {
    let name = ResourceExt::name_any(rc);
    let namespace = ResourceExt::namespace(rc).unwrap_or_default();

    let (desired, current, ready) = if let Some(ref s) = rc.status {
        let desired = rc
            .spec
            .as_ref()
            .and_then(|spec| spec.replicas)
            .unwrap_or(0);
        (
            desired.to_string(),
            s.replicas.to_string(),
            s.ready_replicas.unwrap_or(0).to_string(),
        )
    } else {
        ("0".to_string(), "0".to_string(), "0".to_string())
    };

    let age = format_age(rc.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(rc).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![
            ("desired".to_string(), desired),
            ("current".to_string(), current),
            ("ready".to_string(), ready),
        ],
        raw_yaml,
    }
}

fn job_to_resource_item(job: &Job) -> ResourceItem {
    let name = ResourceExt::name_any(job);
    let namespace = ResourceExt::namespace(job).unwrap_or_default();

    let completions = if let Some(ref s) = job.status {
        let desired = job
            .spec
            .as_ref()
            .and_then(|spec| spec.completions)
            .unwrap_or(1);
        let succeeded = s.succeeded.unwrap_or(0);
        format!("{}/{}", succeeded, desired)
    } else {
        "0/1".to_string()
    };

    let age = format_age(job.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(job).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![("completions".to_string(), completions)],
        raw_yaml,
    }
}

fn cronjob_to_resource_item(cj: &CronJob) -> ResourceItem {
    let name = ResourceExt::name_any(cj);
    let namespace = ResourceExt::namespace(cj).unwrap_or_default();

    let schedule = cj
        .spec
        .as_ref()
        .map(|s| s.schedule.clone())
        .unwrap_or_else(|| "<none>".to_string());
    let suspend = cj
        .spec
        .as_ref()
        .and_then(|s| s.suspend)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "false".to_string());
    let active = cj
        .status
        .as_ref()
        .and_then(|s| s.active.as_ref())
        .map(|a| a.len().to_string())
        .unwrap_or_else(|| "0".to_string());

    let age = format_age(cj.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(cj).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![
            ("schedule".to_string(), schedule),
            ("suspend".to_string(), suspend),
            ("active".to_string(), active),
        ],
        raw_yaml,
    }
}

fn hpa_to_resource_item(hpa: &HorizontalPodAutoscaler) -> ResourceItem {
    let name = ResourceExt::name_any(hpa);
    let namespace = ResourceExt::namespace(hpa).unwrap_or_default();

    let minpods = hpa
        .spec
        .as_ref()
        .and_then(|s| s.min_replicas)
        .map(|v| v.to_string())
        .unwrap_or_else(|| "<unset>".to_string());
    let maxpods = hpa
        .spec
        .as_ref()
        .map(|s| s.max_replicas.to_string())
        .unwrap_or_else(|| "0".to_string());
    let replicas = hpa
        .status
        .as_ref()
        .map(|s| s.current_replicas.to_string())
        .unwrap_or_else(|| "0".to_string());

    let age = format_age(hpa.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(hpa).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![
            ("minpods".to_string(), minpods),
            ("maxpods".to_string(), maxpods),
            ("replicas".to_string(), replicas),
        ],
        raw_yaml,
    }
}

fn service_to_resource_item(svc: &Service) -> ResourceItem {
    let name = ResourceExt::name_any(svc);
    let namespace = ResourceExt::namespace(svc).unwrap_or_default();

    let svc_type = svc
        .spec
        .as_ref()
        .and_then(|s| s.type_.clone())
        .unwrap_or_else(|| "ClusterIP".to_string());
    let cluster_ip = svc
        .spec
        .as_ref()
        .and_then(|s| s.cluster_ip.clone())
        .unwrap_or_else(|| "<none>".to_string());
    let ports = svc
        .spec
        .as_ref()
        .and_then(|s| s.ports.as_ref())
        .map(|ports| {
            ports
                .iter()
                .map(|p| {
                    if let Some(ref name) = p.name {
                        format!("{}:{}/{}", p.port, name, p.protocol.as_deref().unwrap_or("TCP"))
                    } else {
                        format!("{}/{}", p.port, p.protocol.as_deref().unwrap_or("TCP"))
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_else(|| "<none>".to_string());

    let age = format_age(svc.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(svc).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![
            ("type".to_string(), svc_type),
            ("cluster-ip".to_string(), cluster_ip),
            ("ports".to_string(), ports),
        ],
        raw_yaml,
    }
}

fn endpoints_to_resource_item(ep: &Endpoints) -> ResourceItem {
    let name = ResourceExt::name_any(ep);
    let namespace = ResourceExt::namespace(ep).unwrap_or_default();

    let endpoints = ep
        .subsets
        .as_ref()
        .map(|subsets| {
            let addrs: Vec<String> = subsets
                .iter()
                .flat_map(|s| {
                    let addresses = s
                        .addresses
                        .as_ref()
                        .map(|a| a.iter().map(|addr| addr.ip.clone()).collect::<Vec<_>>())
                        .unwrap_or_default();
                    addresses
                })
                .collect();
            if addrs.is_empty() {
                "<none>".to_string()
            } else if addrs.len() > 3 {
                format!("{}... ({} total)", addrs[..3].join(","), addrs.len())
            } else {
                addrs.join(",")
            }
        })
        .unwrap_or_else(|| "<none>".to_string());

    let age = format_age(ep.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(ep).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![("endpoints".to_string(), endpoints)],
        raw_yaml,
    }
}

fn ingress_to_resource_item(ing: &Ingress) -> ResourceItem {
    let name = ResourceExt::name_any(ing);
    let namespace = ResourceExt::namespace(ing).unwrap_or_default();

    let class = ing
        .spec
        .as_ref()
        .and_then(|s| s.ingress_class_name.clone())
        .unwrap_or_else(|| "<none>".to_string());
    let hosts = ing
        .spec
        .as_ref()
        .and_then(|s| s.rules.as_ref())
        .map(|rules| {
            rules
                .iter()
                .filter_map(|r| r.host.clone())
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_else(|| "*".to_string());

    let age = format_age(ing.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(ing).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![
            ("class".to_string(), class),
            ("hosts".to_string(), hosts),
        ],
        raw_yaml,
    }
}

fn network_policy_to_resource_item(np: &NetworkPolicy) -> ResourceItem {
    let name = ResourceExt::name_any(np);
    let namespace = ResourceExt::namespace(np).unwrap_or_default();

    let pod_selector = np
        .spec
        .as_ref()
        .and_then(|s| s.pod_selector.as_ref())
        .and_then(|sel| sel.match_labels.as_ref())
        .map(|labels: &std::collections::BTreeMap<String, String>| {
            labels
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_else(|| "<all>".to_string());

    let age = format_age(np.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(np).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![("pod-selector".to_string(), pod_selector)],
        raw_yaml,
    }
}

fn configmap_to_resource_item(cm: &ConfigMap) -> ResourceItem {
    let name = ResourceExt::name_any(cm);
    let namespace = ResourceExt::namespace(cm).unwrap_or_default();

    let data_count = cm.data.as_ref().map(|d| d.len()).unwrap_or(0)
        + cm.binary_data.as_ref().map(|d| d.len()).unwrap_or(0);

    let age = format_age(cm.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(cm).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![("data".to_string(), data_count.to_string())],
        raw_yaml,
    }
}

fn secret_to_resource_item(secret: &Secret) -> ResourceItem {
    let name = ResourceExt::name_any(secret);
    let namespace = ResourceExt::namespace(secret).unwrap_or_default();

    let secret_type = secret
        .type_
        .clone()
        .unwrap_or_else(|| "Opaque".to_string());
    let data_count = secret.data.as_ref().map(|d| d.len()).unwrap_or(0);

    let age = format_age(secret.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(secret).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![
            ("type".to_string(), secret_type),
            ("data".to_string(), data_count.to_string()),
        ],
        raw_yaml,
    }
}

fn pvc_to_resource_item(pvc: &PersistentVolumeClaim) -> ResourceItem {
    let name = ResourceExt::name_any(pvc);
    let namespace = ResourceExt::namespace(pvc).unwrap_or_default();

    let (status, volume, capacity) = if let Some(ref s) = pvc.status {
        let phase = s.phase.clone().unwrap_or_else(|| "Unknown".to_string());
        let vol = pvc
            .spec
            .as_ref()
            .and_then(|spec| spec.volume_name.clone())
            .unwrap_or_else(|| "<none>".to_string());
        let cap = s
            .capacity
            .as_ref()
            .and_then(|c| c.get("storage"))
            .map(|q| q.0.clone())
            .unwrap_or_else(|| "<none>".to_string());
        (phase, vol, cap)
    } else {
        (
            "Unknown".to_string(),
            "<none>".to_string(),
            "<none>".to_string(),
        )
    };

    let age = format_age(pvc.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(pvc).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status,
        age,
        extra: vec![
            ("volume".to_string(), volume),
            ("capacity".to_string(), capacity),
        ],
        raw_yaml,
    }
}

fn pv_to_resource_item(pv: &PersistentVolume) -> ResourceItem {
    let name = ResourceExt::name_any(pv);
    let namespace = ResourceExt::namespace(pv).unwrap_or_default();

    let capacity = pv
        .spec
        .as_ref()
        .and_then(|s| s.capacity.as_ref())
        .and_then(|c| c.get("storage"))
        .map(|q| q.0.clone())
        .unwrap_or_else(|| "<none>".to_string());
    let status = pv
        .status
        .as_ref()
        .and_then(|s| s.phase.clone())
        .unwrap_or_else(|| "Unknown".to_string());
    let storageclass = pv
        .spec
        .as_ref()
        .and_then(|s| s.storage_class_name.clone())
        .unwrap_or_else(|| "<none>".to_string());

    let age = format_age(pv.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(pv).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status,
        age,
        extra: vec![
            ("capacity".to_string(), capacity),
            ("storageclass".to_string(), storageclass),
        ],
        raw_yaml,
    }
}

fn storageclass_to_resource_item(sc: &StorageClass) -> ResourceItem {
    let name = ResourceExt::name_any(sc);
    let namespace = ResourceExt::namespace(sc).unwrap_or_default();

    let provisioner = sc.provisioner.clone();

    let age = format_age(sc.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(sc).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![("provisioner".to_string(), provisioner)],
        raw_yaml,
    }
}

fn serviceaccount_to_resource_item(sa: &ServiceAccount) -> ResourceItem {
    let name = ResourceExt::name_any(sa);
    let namespace = ResourceExt::namespace(sa).unwrap_or_default();

    let age = format_age(sa.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(sa).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![],
        raw_yaml,
    }
}

fn namespace_to_resource_item(ns: &Namespace) -> ResourceItem {
    let name = ResourceExt::name_any(ns);

    let status = ns
        .status
        .as_ref()
        .and_then(|s| s.phase.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let age = format_age(ns.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(ns).unwrap_or_default();

    ResourceItem {
        name,
        namespace: String::new(),
        status,
        age,
        extra: vec![],
        raw_yaml,
    }
}

fn node_to_resource_item(node: &Node) -> ResourceItem {
    let name = ResourceExt::name_any(node);

    let status = node
        .status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .and_then(|conds| {
            conds
                .iter()
                .find(|c| c.type_ == "Ready")
                .map(|c| {
                    if c.status == "True" {
                        "Ready".to_string()
                    } else {
                        "NotReady".to_string()
                    }
                })
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let roles = node
        .metadata
        .labels
        .as_ref()
        .map(|labels| {
            labels
                .keys()
                .filter_map(|k| {
                    k.strip_prefix("node-role.kubernetes.io/")
                        .map(|r| r.to_string())
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    let roles = if roles.is_empty() {
        "<none>".to_string()
    } else {
        roles
    };

    let age = format_age(node.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(node).unwrap_or_default();

    ResourceItem {
        name,
        namespace: String::new(),
        status,
        age,
        extra: vec![("roles".to_string(), roles)],
        raw_yaml,
    }
}

fn event_to_resource_item(ev: &Event) -> ResourceItem {
    let name = ev
        .involved_object
        .name
        .clone()
        .unwrap_or_else(|| ResourceExt::name_any(ev));
    let namespace = ResourceExt::namespace(ev).unwrap_or_default();

    let ev_type = ev
        .type_
        .clone()
        .unwrap_or_else(|| "Normal".to_string());
    let reason = ev
        .reason
        .clone()
        .unwrap_or_else(|| "<none>".to_string());
    let message = ev
        .message
        .clone()
        .unwrap_or_else(|| "<none>".to_string());

    let age = format_age(ev.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(ev).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![
            ("type".to_string(), ev_type),
            ("reason".to_string(), reason),
            ("message".to_string(), message),
        ],
        raw_yaml,
    }
}

fn resourcequota_to_resource_item(rq: &ResourceQuota) -> ResourceItem {
    let name = ResourceExt::name_any(rq);
    let namespace = ResourceExt::namespace(rq).unwrap_or_default();
    let age = format_age(rq.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(rq).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![],
        raw_yaml,
    }
}

fn limitrange_to_resource_item(lr: &LimitRange) -> ResourceItem {
    let name = ResourceExt::name_any(lr);
    let namespace = ResourceExt::namespace(lr).unwrap_or_default();
    let age = format_age(lr.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(lr).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![],
        raw_yaml,
    }
}

fn pdb_to_resource_item(pdb: &PodDisruptionBudget) -> ResourceItem {
    let name = ResourceExt::name_any(pdb);
    let namespace = ResourceExt::namespace(pdb).unwrap_or_default();

    let min_available = pdb
        .spec
        .as_ref()
        .and_then(|s| s.min_available.as_ref())
        .map(|v| match v {
            k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(i) => i.to_string(),
            k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::String(s) => s.clone(),
        })
        .unwrap_or_else(|| "N/A".to_string());
    let max_unavailable = pdb
        .spec
        .as_ref()
        .and_then(|s| s.max_unavailable.as_ref())
        .map(|v| match v {
            k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(i) => i.to_string(),
            k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::String(s) => s.clone(),
        })
        .unwrap_or_else(|| "N/A".to_string());

    let age = format_age(pdb.metadata.creation_timestamp.as_ref());
    let raw_yaml = serde_yaml::to_string(pdb).unwrap_or_default();

    ResourceItem {
        name,
        namespace,
        status: String::new(),
        age,
        extra: vec![
            ("min-available".to_string(), min_available),
            ("max-unavailable".to_string(), max_unavailable),
        ],
        raw_yaml,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_age(timestamp: Option<&Time>) -> String {
    let Some(ts) = timestamp else {
        return "<unknown>".to_string();
    };

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let ts_secs = ts.0.as_second();
    let diff_secs = now_secs - ts_secs;

    if diff_secs < 0 {
        return "0s".to_string();
    }

    let days = diff_secs / 86400;
    let hours = (diff_secs % 86400) / 3600;
    let minutes = (diff_secs % 3600) / 60;
    let seconds = diff_secs % 60;

    if days > 0 {
        format!("{}d{}h", days, hours)
    } else if hours > 0 {
        format!("{}h{}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        format!("{}s", seconds)
    }
}
