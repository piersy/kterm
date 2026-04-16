use std::time::Duration;

pub mod actions;
pub mod client;
pub mod logs;
pub mod resources;

/// Shared timeout for all Kubernetes API interactions (list, get, describe,
/// count, log-stream connect, cluster connectivity probe).
pub const K8S_TIMEOUT: Duration = Duration::from_secs(3);
