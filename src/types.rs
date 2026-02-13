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
