use std::time::Duration;

use anyhow::{Context, Result};
use futures::AsyncBufReadExt;
use futures::TryStreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::api::LogParams;
use kube::{Api, Client};
use tokio::sync::mpsc;

use crate::event::{self, AppEvent};

pub const LOG_STREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(1);

pub async fn stream_pod_logs(
    client: Client,
    namespace: &str,
    pod_name: &str,
    container: Option<&str>,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let api: Api<Pod> = Api::namespaced(client, namespace);

    let mut params = LogParams {
        follow: true,
        tail_lines: Some(100),
        ..Default::default()
    };

    if let Some(c) = container {
        params.container = Some(c.to_string());
    }

    let stream = tokio::time::timeout(
        LOG_STREAM_CONNECT_TIMEOUT,
        api.log_stream(pod_name, &params),
    )
    .await
    .context("log stream timed out (cluster may be unreachable)")?
    .context("failed to open log stream")?;

    let mut lines = stream.lines();

    while let Some(line) = lines.try_next().await? {
        if tx.send(AppEvent::LogLine(line)).is_err() {
            crate::logging::log_error("Log stream: event channel closed");
            break;
        }
    }

    event::send_event(&tx, AppEvent::LogStreamEnded);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_stream_timeout_is_reasonable() {
        assert!(LOG_STREAM_CONNECT_TIMEOUT.as_secs() >= 1);
        assert!(LOG_STREAM_CONNECT_TIMEOUT.as_secs() <= 5);
    }
}
