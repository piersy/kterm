use anyhow::{Context, Result};
use futures::AsyncBufReadExt;
use futures::TryStreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::api::LogParams;
use kube::{Api, Client};
use tokio::sync::mpsc;

use crate::event::AppEvent;

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

    let stream = api
        .log_stream(pod_name, &params)
        .await
        .context("Failed to open log stream")?;

    let mut lines = stream.lines();

    while let Some(line) = lines.try_next().await? {
        if tx.send(AppEvent::LogLine(line)).is_err() {
            break;
        }
    }

    let _ = tx.send(AppEvent::LogStreamEnded);

    Ok(())
}
