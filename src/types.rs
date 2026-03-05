use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    // Workloads
    Pods,
    Deployments,
    StatefulSets,
    DaemonSets,
    ReplicaSets,
    ReplicationControllers,
    Jobs,
    CronJobs,
    HorizontalPodAutoscalers,

    // Service & Networking
    Services,
    Endpoints,
    Ingresses,
    NetworkPolicies,

    // Config & Storage
    ConfigMaps,
    Secrets,
    PersistentVolumeClaims,
    PersistentVolumes,
    StorageClasses,

    // Auth
    ServiceAccounts,

    // Cluster
    Namespaces,
    Nodes,
    Events,
    ResourceQuotas,
    LimitRanges,
    PodDisruptionBudgets,
}

impl ResourceType {
    pub const ALL: [ResourceType; 25] = [
        ResourceType::Pods,
        ResourceType::Deployments,
        ResourceType::StatefulSets,
        ResourceType::DaemonSets,
        ResourceType::ReplicaSets,
        ResourceType::ReplicationControllers,
        ResourceType::Jobs,
        ResourceType::CronJobs,
        ResourceType::HorizontalPodAutoscalers,
        ResourceType::Services,
        ResourceType::Endpoints,
        ResourceType::Ingresses,
        ResourceType::NetworkPolicies,
        ResourceType::ConfigMaps,
        ResourceType::Secrets,
        ResourceType::PersistentVolumeClaims,
        ResourceType::PersistentVolumes,
        ResourceType::StorageClasses,
        ResourceType::ServiceAccounts,
        ResourceType::Namespaces,
        ResourceType::Nodes,
        ResourceType::Events,
        ResourceType::ResourceQuotas,
        ResourceType::LimitRanges,
        ResourceType::PodDisruptionBudgets,
    ];

    pub fn column_headers(&self) -> Vec<&'static str> {
        match self {
            ResourceType::Pods => vec!["NAME", "STATUS", "AGE", "RESTARTS", "NODE"],
            ResourceType::Deployments => {
                vec!["NAME", "READY", "UP-TO-DATE", "AVAILABLE", "AGE"]
            }
            ResourceType::StatefulSets => vec!["NAME", "READY", "AGE"],
            ResourceType::DaemonSets => vec!["NAME", "DESIRED", "CURRENT", "READY", "AGE"],
            ResourceType::ReplicaSets => vec!["NAME", "DESIRED", "CURRENT", "READY", "AGE"],
            ResourceType::ReplicationControllers => {
                vec!["NAME", "DESIRED", "CURRENT", "READY", "AGE"]
            }
            ResourceType::Jobs => vec!["NAME", "COMPLETIONS", "AGE"],
            ResourceType::CronJobs => vec!["NAME", "SCHEDULE", "SUSPEND", "ACTIVE", "AGE"],
            ResourceType::HorizontalPodAutoscalers => {
                vec!["NAME", "MINPODS", "MAXPODS", "REPLICAS", "AGE"]
            }
            ResourceType::Services => vec!["NAME", "TYPE", "CLUSTER-IP", "PORTS", "AGE"],
            ResourceType::Endpoints => vec!["NAME", "ENDPOINTS", "AGE"],
            ResourceType::Ingresses => vec!["NAME", "CLASS", "HOSTS", "AGE"],
            ResourceType::NetworkPolicies => vec!["NAME", "POD-SELECTOR", "AGE"],
            ResourceType::ConfigMaps => vec!["NAME", "DATA", "AGE"],
            ResourceType::Secrets => vec!["NAME", "TYPE", "DATA", "AGE"],
            ResourceType::PersistentVolumeClaims => {
                vec!["NAME", "STATUS", "VOLUME", "CAPACITY", "AGE"]
            }
            ResourceType::PersistentVolumes => {
                vec!["NAME", "CAPACITY", "STATUS", "STORAGECLASS", "AGE"]
            }
            ResourceType::StorageClasses => vec!["NAME", "PROVISIONER", "AGE"],
            ResourceType::ServiceAccounts => vec!["NAME", "AGE"],
            ResourceType::Namespaces => vec!["NAME", "STATUS", "AGE"],
            ResourceType::Nodes => vec!["NAME", "STATUS", "ROLES", "AGE"],
            ResourceType::Events => vec!["NAME", "TYPE", "REASON", "MESSAGE", "AGE"],
            ResourceType::ResourceQuotas => vec!["NAME", "AGE"],
            ResourceType::LimitRanges => vec!["NAME", "AGE"],
            ResourceType::PodDisruptionBudgets => {
                vec!["NAME", "MIN-AVAILABLE", "MAX-UNAVAILABLE", "AGE"]
            }
        }
    }

    /// Returns true for cluster-scoped resources (not namespaced).
    pub fn is_cluster_scoped(&self) -> bool {
        matches!(
            self,
            ResourceType::Nodes
                | ResourceType::PersistentVolumes
                | ResourceType::StorageClasses
                | ResourceType::Namespaces
        )
    }
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceType::Pods => write!(f, "po"),
            ResourceType::Deployments => write!(f, "deploy"),
            ResourceType::StatefulSets => write!(f, "sts"),
            ResourceType::DaemonSets => write!(f, "ds"),
            ResourceType::ReplicaSets => write!(f, "rs"),
            ResourceType::ReplicationControllers => write!(f, "rc"),
            ResourceType::Jobs => write!(f, "jobs"),
            ResourceType::CronJobs => write!(f, "cj"),
            ResourceType::HorizontalPodAutoscalers => write!(f, "hpa"),
            ResourceType::Services => write!(f, "svc"),
            ResourceType::Endpoints => write!(f, "ep"),
            ResourceType::Ingresses => write!(f, "ing"),
            ResourceType::NetworkPolicies => write!(f, "netpol"),
            ResourceType::ConfigMaps => write!(f, "cm"),
            ResourceType::Secrets => write!(f, "secrets"),
            ResourceType::PersistentVolumeClaims => write!(f, "pvc"),
            ResourceType::PersistentVolumes => write!(f, "pv"),
            ResourceType::StorageClasses => write!(f, "sc"),
            ResourceType::ServiceAccounts => write!(f, "sa"),
            ResourceType::Namespaces => write!(f, "ns"),
            ResourceType::Nodes => write!(f, "no"),
            ResourceType::Events => write!(f, "ev"),
            ResourceType::ResourceQuotas => write!(f, "quota"),
            ResourceType::LimitRanges => write!(f, "limits"),
            ResourceType::PodDisruptionBudgets => write!(f, "pdb"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    List,
    Detail,
    Logs,
    Confirm(ConfirmAction),
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAction {
    Delete,
    Restart,
}

impl fmt::Display for ConfirmAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfirmAction::Delete => write!(f, "Delete"),
            ConfirmAction::Restart => write!(f, "Restart"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    ContextSelector,
    NamespaceSelector,
    ResourceTypeSelector,
    ResourceList,
}

impl Focus {
    pub fn next(self) -> Self {
        match self {
            Focus::ContextSelector => Focus::NamespaceSelector,
            Focus::NamespaceSelector => Focus::ResourceTypeSelector,
            Focus::ResourceTypeSelector => Focus::ResourceList,
            Focus::ResourceList => Focus::ContextSelector,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Focus::ContextSelector => Focus::ResourceList,
            Focus::NamespaceSelector => Focus::ContextSelector,
            Focus::ResourceTypeSelector => Focus::NamespaceSelector,
            Focus::ResourceList => Focus::ResourceTypeSelector,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResourceItem {
    pub name: String,
    pub namespace: String,
    pub status: String,
    pub age: String,
    pub extra: Vec<(String, String)>,
    pub raw_yaml: String,
}

impl ResourceItem {
    /// Returns column values matching the headers for the given resource type.
    pub fn columns(&self, resource_type: ResourceType) -> Vec<String> {
        resource_type
            .column_headers()
            .iter()
            .map(|h| {
                let key = h.to_lowercase();
                match key.as_str() {
                    "name" => self.name.clone(),
                    "status" | "phase" => self.status.clone(),
                    "age" => self.age.clone(),
                    "namespace" => self.namespace.clone(),
                    _ => self.extra_val(&key),
                }
            })
            .collect()
    }

    fn extra_val(&self, key: &str) -> String {
        self.extra
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| "<none>".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub resource: ResourceItem,
    pub context: String,
    pub resource_type: ResourceType,
}

/// Fuzzy subsequence match. Returns a score if all characters in `query`
/// appear in order within `target`, or None if they don't.
pub fn fuzzy_match(query: &str, target: &str) -> Option<i64> {
    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let target_lower: Vec<char> = target.to_lowercase().chars().collect();

    if query_lower.is_empty() {
        return Some(0);
    }

    let mut qi = 0;
    let mut score: i64 = 0;
    let mut prev_matched = false;

    for (ti, &tc) in target_lower.iter().enumerate() {
        if qi < query_lower.len() && tc == query_lower[qi] {
            score += 1;
            // Consecutive match bonus
            if prev_matched {
                score += 2;
            }
            // Word boundary bonus (start of string, after - or _ or /)
            if ti == 0
                || matches!(
                    target_lower.get(ti.wrapping_sub(1)),
                    Some('-') | Some('_') | Some('/')
                )
            {
                score += 3;
            }
            prev_matched = true;
            qi += 1;
        } else {
            prev_matched = false;
        }
    }

    if qi == query_lower.len() {
        // Bonus for shorter targets (more precise match)
        score += (100 - target_lower.len() as i64).max(0);
        Some(score)
    } else {
        None
    }
}
