use std::collections::BTreeMap;

use anyhow::Result;
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::apps::v1::StatefulSet;
use k8s_openapi::api::core::v1::{Event, PersistentVolumeClaim, Pod};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kube::api::ListParams;
use kube::runtime::watcher;
use kube::runtime::WatchStreamExt;
use kube::{Api, Client, ResourceExt};
use tokio::sync::mpsc;

use crate::event::AppEvent;
use crate::types::{ResourceItem, ResourceType};

pub async fn watch_resources(
    client: Client,
    namespace: &str,
    resource_type: ResourceType,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    match resource_type {
        ResourceType::Pods => watch_pods(client, namespace, tx).await,
        ResourceType::PersistentVolumeClaims => watch_pvcs(client, namespace, tx).await,
        ResourceType::StatefulSets => watch_statefulsets(client, namespace, tx).await,
    }
}

async fn watch_pods(
    client: Client,
    namespace: &str,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let api: Api<Pod> = Api::namespaced(client, namespace);
    let mut stream = watcher(api, watcher::Config::default())
        .default_backoff()
        .applied_objects()
        .boxed();

    let mut cache: BTreeMap<String, Pod> = BTreeMap::new();

    while let Some(pod) = stream.try_next().await? {
        let name = ResourceExt::name_any(&pod);
        let ns = ResourceExt::namespace(&pod).unwrap_or_default();
        let key = format!("{}/{}", ns, name);
        cache.insert(key, pod);

        let items: Vec<ResourceItem> = cache.values().map(pod_to_resource_item).collect();
        if tx.send(AppEvent::ResourcesUpdated(items)).is_err() {
            break;
        }
    }

    Ok(())
}

async fn watch_pvcs(
    client: Client,
    namespace: &str,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let api: Api<PersistentVolumeClaim> = Api::namespaced(client, namespace);
    let mut stream = watcher(api, watcher::Config::default())
        .default_backoff()
        .applied_objects()
        .boxed();

    let mut cache: BTreeMap<String, PersistentVolumeClaim> = BTreeMap::new();

    while let Some(pvc) = stream.try_next().await? {
        let name = ResourceExt::name_any(&pvc);
        let ns = ResourceExt::namespace(&pvc).unwrap_or_default();
        let key = format!("{}/{}", ns, name);
        cache.insert(key, pvc);

        let items: Vec<ResourceItem> = cache.values().map(pvc_to_resource_item).collect();
        if tx.send(AppEvent::ResourcesUpdated(items)).is_err() {
            break;
        }
    }

    Ok(())
}

async fn watch_statefulsets(
    client: Client,
    namespace: &str,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let api: Api<StatefulSet> = Api::namespaced(client, namespace);
    let mut stream = watcher(api, watcher::Config::default())
        .default_backoff()
        .applied_objects()
        .boxed();

    let mut cache: BTreeMap<String, StatefulSet> = BTreeMap::new();

    while let Some(ss) = stream.try_next().await? {
        let name = ResourceExt::name_any(&ss);
        let ns = ResourceExt::namespace(&ss).unwrap_or_default();
        let key = format!("{}/{}", ns, name);
        cache.insert(key, ss);

        let items: Vec<ResourceItem> = cache.values().map(statefulset_to_resource_item).collect();
        if tx.send(AppEvent::ResourcesUpdated(items)).is_err() {
            break;
        }
    }

    Ok(())
}

pub async fn list_all_resources(
    client: Client,
    resource_type: ResourceType,
) -> Result<Vec<ResourceItem>> {
    match resource_type {
        ResourceType::Pods => {
            let api: Api<Pod> = Api::all(client);
            let list = api.list(&ListParams::default()).await?;
            Ok(list.items.iter().map(pod_to_resource_item).collect())
        }
        ResourceType::PersistentVolumeClaims => {
            let api: Api<PersistentVolumeClaim> = Api::all(client);
            let list = api.list(&ListParams::default()).await?;
            Ok(list.items.iter().map(pvc_to_resource_item).collect())
        }
        ResourceType::StatefulSets => {
            let api: Api<StatefulSet> = Api::all(client);
            let list = api.list(&ListParams::default()).await?;
            Ok(list.items.iter().map(statefulset_to_resource_item).collect())
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
        ResourceType::Pods => describe_pod(client, namespace, name).await,
        ResourceType::PersistentVolumeClaims => describe_pvc(client, namespace, name).await,
        ResourceType::StatefulSets => describe_statefulset(client, namespace, name).await,
    }
}

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
                desc.push_str(&format!(
                    "    Restarts: {}\n",
                    cs.restart_count
                ));
            }
        }
    }

    if let Some(ref spec) = pod.spec {
        if let Some(ref node_name) = spec.node_name {
            desc.push_str(&format!("\nNode:         {}\n", node_name));
        }
    }

    // Fetch events
    let events = fetch_events(client, namespace, name).await;
    if !events.is_empty() {
        desc.push_str("\nEvents:\n");
        for event in &events {
            desc.push_str(&format!("  {}\n", event));
        }
    }

    // Full YAML
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
        desc.push_str(&format!(
            "Replicas:     {}\n",
            status.replicas
        ));
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

fn pod_to_resource_item(pod: &Pod) -> ResourceItem {
    let name = ResourceExt::name_any(pod);
    let namespace = ResourceExt::namespace(pod).unwrap_or_default();

    let (status, restarts, node) = if let Some(ref s) = pod.status {
        let phase = s.phase.clone().unwrap_or_else(|| "Unknown".to_string());

        // Check container statuses for more specific status
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
