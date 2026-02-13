use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Pods,
    PersistentVolumeClaims,
    StatefulSets,
}

impl ResourceType {
    pub const ALL: [ResourceType; 3] = [
        ResourceType::Pods,
        ResourceType::PersistentVolumeClaims,
        ResourceType::StatefulSets,
    ];

    pub fn next(self) -> Self {
        match self {
            ResourceType::Pods => ResourceType::PersistentVolumeClaims,
            ResourceType::PersistentVolumeClaims => ResourceType::StatefulSets,
            ResourceType::StatefulSets => ResourceType::Pods,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            ResourceType::Pods => ResourceType::StatefulSets,
            ResourceType::PersistentVolumeClaims => ResourceType::Pods,
            ResourceType::StatefulSets => ResourceType::PersistentVolumeClaims,
        }
    }

    pub fn column_headers(&self) -> Vec<&'static str> {
        match self {
            ResourceType::Pods => vec!["NAME", "STATUS", "AGE", "RESTARTS", "NODE"],
            ResourceType::PersistentVolumeClaims => {
                vec!["NAME", "STATUS", "VOLUME", "CAPACITY", "AGE"]
            }
            ResourceType::StatefulSets => vec!["NAME", "READY", "AGE"],
        }
    }
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceType::Pods => write!(f, "Pods"),
            ResourceType::PersistentVolumeClaims => write!(f, "PVCs"),
            ResourceType::StatefulSets => write!(f, "StatefulSets"),
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
        match resource_type {
            ResourceType::Pods => {
                let restarts = self.extra_val("restarts");
                let node = self.extra_val("node");
                vec![
                    self.name.clone(),
                    self.status.clone(),
                    self.age.clone(),
                    restarts,
                    node,
                ]
            }
            ResourceType::PersistentVolumeClaims => {
                let volume = self.extra_val("volume");
                let capacity = self.extra_val("capacity");
                vec![
                    self.name.clone(),
                    self.status.clone(),
                    volume,
                    capacity,
                    self.age.clone(),
                ]
            }
            ResourceType::StatefulSets => {
                let ready = self.extra_val("ready");
                vec![self.name.clone(), ready, self.age.clone()]
            }
        }
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
