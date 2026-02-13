use anyhow::{Context, Result};
use k8s_openapi::api::core::v1::Namespace;
use kube::api::ListParams;
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Api, Client, Config};

pub struct K8sManager {
    kubeconfig: Kubeconfig,
    pub current_context: String,
    pub client: Client,
}

impl K8sManager {
    pub async fn new() -> Result<Self> {
        let kubeconfig = Kubeconfig::read().context("Failed to read kubeconfig")?;
        let current_context = kubeconfig
            .current_context
            .clone()
            .unwrap_or_default();

        let config = Config::from_kubeconfig(&KubeConfigOptions {
            context: Some(current_context.clone()),
            ..Default::default()
        })
        .await
        .context("Failed to create config from kubeconfig")?;

        let client = Client::try_from(config).context("Failed to create Kubernetes client")?;

        Ok(Self {
            kubeconfig,
            current_context,
            client,
        })
    }

    pub fn context_names(&self) -> Vec<String> {
        self.kubeconfig
            .contexts
            .iter()
            .map(|c| c.name.clone())
            .collect()
    }

    pub async fn switch_context(&mut self, context_name: &str) -> Result<()> {
        let config = Config::from_kubeconfig(&KubeConfigOptions {
            context: Some(context_name.to_string()),
            ..Default::default()
        })
        .await
        .context("Failed to create config for context")?;

        self.client = Client::try_from(config).context("Failed to create client")?;
        self.current_context = context_name.to_string();
        Ok(())
    }

    pub async fn client_for_context(context_name: &str) -> Result<Client> {
        let _kubeconfig = Kubeconfig::read().context("Failed to read kubeconfig")?;
        let config = Config::from_kubeconfig(&KubeConfigOptions {
            context: Some(context_name.to_string()),
            ..Default::default()
        })
        .await
        .context("Failed to create config for context")?;

        Client::try_from(config).context("Failed to create client for context")
    }

    pub async fn list_namespaces(&self) -> Result<Vec<String>> {
        let ns_api: Api<Namespace> = Api::all(self.client.clone());
        let ns_list = ns_api
            .list(&ListParams::default())
            .await
            .context("Failed to list namespaces")?;

        let mut names: Vec<String> = ns_list
            .items
            .iter()
            .filter_map(|ns| ns.metadata.name.clone())
            .collect();
        names.sort();
        Ok(names)
    }
}
