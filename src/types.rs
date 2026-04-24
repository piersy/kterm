use std::fmt;

/// Sentinel label representing "all namespaces". When this is the selected
/// namespace, K8s queries are scoped cluster-wide via `Api::all` instead of
/// to a single namespace. The label contains characters (spaces, angle
/// brackets) that cannot appear in a real DNS-1123 Kubernetes namespace name,
/// so it is guaranteed not to collide with a real namespace.
pub const ALL_NAMESPACES_LABEL: &str = "<all namespaces>";

/// Returns true if the given namespace string is the all-namespaces sentinel.
pub fn is_all_namespaces(ns: &str) -> bool {
    ns == ALL_NAMESPACES_LABEL
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

    /// Returns the unified column definitions for this resource type.
    ///
    /// Each [`ColumnDef`] bundles the header label, relative width, and
    /// whether the column should receive status-aware coloring.  When
    /// `all_namespaces` is true a NAMESPACE column is injected after NAME
    /// so the user can distinguish resources across namespaces.
    pub fn column_defs(&self, all_namespaces: bool) -> Vec<ColumnDef> {
        use ColumnDef as C;

        let mut defs = match self {
            ResourceType::Pods => vec![
                C::name(30),
                C::status(15),
                C::col("AGE", 15),
                C::col("RESTARTS", 15),
                C::col("NODE", 25),
            ],
            ResourceType::Deployments => vec![
                C::name(30),
                C::col("READY", 15),
                C::col("UP-TO-DATE", 20),
                C::col("AVAILABLE", 20),
                C::col("AGE", 15),
            ],
            ResourceType::StatefulSets => vec![
                C::name(40),
                C::col("READY", 30),
                C::col("AGE", 30),
            ],
            ResourceType::DaemonSets
            | ResourceType::ReplicaSets
            | ResourceType::ReplicationControllers => vec![
                C::name(30),
                C::col("DESIRED", 15),
                C::col("CURRENT", 15),
                C::col("READY", 15),
                C::col("AGE", 25),
            ],
            ResourceType::Jobs => vec![
                C::name(40),
                C::col("COMPLETIONS", 30),
                C::col("AGE", 30),
            ],
            ResourceType::CronJobs => vec![
                C::name(25),
                C::col("SCHEDULE", 25),
                C::col("SUSPEND", 15),
                C::col("ACTIVE", 15),
                C::col("AGE", 20),
            ],
            ResourceType::HorizontalPodAutoscalers => vec![
                C::name(30),
                C::col("MINPODS", 15),
                C::col("MAXPODS", 15),
                C::col("REPLICAS", 15),
                C::col("AGE", 25),
            ],
            ResourceType::Services => vec![
                C::name(25),
                C::col("TYPE", 15),
                C::col("CLUSTER-IP", 20),
                C::col("PORTS", 25),
                C::col("AGE", 15),
            ],
            ResourceType::Endpoints => vec![
                C::name(30),
                C::col("ENDPOINTS", 50),
                C::col("AGE", 20),
            ],
            ResourceType::Ingresses => vec![
                C::name(25),
                C::col("CLASS", 20),
                C::col("HOSTS", 35),
                C::col("AGE", 20),
            ],
            ResourceType::NetworkPolicies => vec![
                C::name(30),
                C::col("POD-SELECTOR", 50),
                C::col("AGE", 20),
            ],
            ResourceType::ConfigMaps => vec![
                C::name(50),
                C::col("DATA", 20),
                C::col("AGE", 30),
            ],
            ResourceType::Secrets => vec![
                C::name(30),
                C::col("TYPE", 30),
                C::col("DATA", 15),
                C::col("AGE", 25),
            ],
            ResourceType::PersistentVolumeClaims => vec![
                C::name(25),
                C::status(15),
                C::col("VOLUME", 25),
                C::col("CAPACITY", 15),
                C::col("AGE", 20),
            ],
            ResourceType::PersistentVolumes => vec![
                C::name(25),
                C::col("CAPACITY", 15),
                C::status(15),
                C::col("STORAGECLASS", 25),
                C::col("AGE", 20),
            ],
            ResourceType::StorageClasses => vec![
                C::name(30),
                C::col("PROVISIONER", 50),
                C::col("AGE", 20),
            ],
            ResourceType::ServiceAccounts
            | ResourceType::ResourceQuotas
            | ResourceType::LimitRanges => vec![
                C::name(60),
                C::col("AGE", 40),
            ],
            ResourceType::Namespaces => vec![
                C::name(40),
                C::status(30),
                C::col("AGE", 30),
            ],
            ResourceType::Nodes => vec![
                C::name(30),
                C::status(20),
                C::col("ROLES", 25),
                C::col("AGE", 25),
            ],
            ResourceType::Events => vec![
                C::name(20),
                C::col("TYPE", 10),
                C::col("REASON", 15),
                C::col("MESSAGE", 40),
                C::col("AGE", 15),
            ],
            ResourceType::PodDisruptionBudgets => vec![
                C::name(30),
                C::col("MIN-AVAILABLE", 25),
                C::col("MAX-UNAVAILABLE", 25),
                C::col("AGE", 20),
            ],
        };

        if all_namespaces {
            // Insert NAMESPACE column right after NAME.
            let ns_col = C::col("NAMESPACE", 15);
            let insert_pos = defs.iter().position(|d| d.header == "NAME")
                .map(|i| i + 1)
                .unwrap_or(1);
            defs.insert(insert_pos, ns_col);
        }

        defs
    }

    /// Returns true if this resource type supports viewing logs.
    pub fn supports_logs(&self) -> bool {
        matches!(self, ResourceType::Pods)
    }

    /// Returns true if this resource type supports `exec` into a container.
    pub fn supports_exec(&self) -> bool {
        matches!(self, ResourceType::Pods)
    }

    /// Returns true if this resource type supports restart.
    pub fn supports_restart(&self) -> bool {
        matches!(
            self,
            ResourceType::Pods
                | ResourceType::Deployments
                | ResourceType::StatefulSets
                | ResourceType::DaemonSets
        )
    }

    #[allow(dead_code)]
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
            ResourceType::Pods => write!(f, "pods"),
            ResourceType::Deployments => write!(f, "deployments"),
            ResourceType::StatefulSets => write!(f, "statefulsets"),
            ResourceType::DaemonSets => write!(f, "daemonsets"),
            ResourceType::ReplicaSets => write!(f, "replicasets"),
            ResourceType::ReplicationControllers => write!(f, "replicationcontrollers"),
            ResourceType::Jobs => write!(f, "jobs"),
            ResourceType::CronJobs => write!(f, "cronjobs"),
            ResourceType::HorizontalPodAutoscalers => write!(f, "horizontalpodautoscalers"),
            ResourceType::Services => write!(f, "services"),
            ResourceType::Endpoints => write!(f, "endpoints"),
            ResourceType::Ingresses => write!(f, "ingresses"),
            ResourceType::NetworkPolicies => write!(f, "networkpolicies"),
            ResourceType::ConfigMaps => write!(f, "configmaps"),
            ResourceType::Secrets => write!(f, "secrets"),
            ResourceType::PersistentVolumeClaims => write!(f, "persistentvolumeclaims"),
            ResourceType::PersistentVolumes => write!(f, "persistentvolumes"),
            ResourceType::StorageClasses => write!(f, "storageclasses"),
            ResourceType::ServiceAccounts => write!(f, "serviceaccounts"),
            ResourceType::Namespaces => write!(f, "namespaces"),
            ResourceType::Nodes => write!(f, "nodes"),
            ResourceType::Events => write!(f, "events"),
            ResourceType::ResourceQuotas => write!(f, "resourcequotas"),
            ResourceType::LimitRanges => write!(f, "limitranges"),
            ResourceType::PodDisruptionBudgets => write!(f, "poddisruptionbudgets"),
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

/// Which selector is currently active (open). None means focus is on the main resource list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorTarget {
    Context,
    Namespace,
    ResourceType,
}

/// Focus state: either on the resource list or on a selector overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    ResourceList,
    Selector(SelectorTarget),
}

/// Defines a single column in the resource-list table.
///
/// Bundles the header label, a relative width weight, and whether the
/// column should receive status-aware coloring.
#[derive(Debug, Clone)]
pub struct ColumnDef {
    /// Header label displayed at the top of the column.
    pub header: &'static str,
    /// Relative width weight (not a raw percentage). The renderer
    /// normalises all weights to `Constraint::Percentage` values that
    /// sum to 100.
    pub width: u16,
    /// If true the renderer applies `status_style` colouring to cells
    /// in this column.
    pub is_status: bool,
}

impl ColumnDef {
    /// A regular (non-status) column.
    pub const fn col(header: &'static str, width: u16) -> Self {
        Self { header, width, is_status: false }
    }

    /// The NAME column (always comes first).
    pub const fn name(width: u16) -> Self {
        Self { header: "NAME", width, is_status: false }
    }

    /// A STATUS column that gets status-aware colouring.
    pub const fn status(width: u16) -> Self {
        Self { header: "STATUS", width, is_status: true }
    }

    /// Convert a slice of [`ColumnDef`] width-weights into normalised
    /// percentage constraints that sum to 100.
    pub fn to_constraints(defs: &[ColumnDef]) -> Vec<ratatui::layout::Constraint> {
        let total: u32 = defs.iter().map(|d| d.width as u32).sum();
        if total == 0 {
            return defs.iter().map(|_| ratatui::layout::Constraint::Percentage(0)).collect();
        }
        defs.iter()
            .map(|d| ratatui::layout::Constraint::Percentage(
                (d.width as u32 * 100 / total) as u16,
            ))
            .collect()
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
    /// Returns column values driven by an arbitrary slice of [`ColumnDef`]s.
    ///
    /// This is the primary column-value resolver. The renderer calls it
    /// with the context-aware defs returned by
    /// [`ResourceType::column_defs`].
    pub fn column_values(&self, defs: &[ColumnDef]) -> Vec<String> {
        defs.iter()
            .map(|d| {
                let key = d.header.to_lowercase();
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
