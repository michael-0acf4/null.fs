use crate::netfs::any_fs::AnyFs;
use eyre::Context;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub name: String,
    pub password: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RelayNode {
    pub address: String,
    pub auth: User,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NodeConfig {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub refresh_secs: Option<u16>,
    #[serde(default)]
    pub users: Vec<User>,
    pub relay_nodes: IndexMap<String, RelayNode>,
    pub volumes: Vec<AnyFs>,
}

impl NodeConfig {
    pub async fn load_from_file(path: &Path) -> eyre::Result<Self> {
        let content = tokio::fs::read_to_string(path)
            .await
            .wrap_err_with(|| format!("Loading configuration file at {}", path.display()))?;

        serde_yaml::from_str(&content).wrap_err_with(|| format!("Parsing configuration file"))
    }

    pub fn resolve_alias(&self, value: &str) -> eyre::Result<RelayNode> {
        self.relay_nodes
            .get(value)
            .cloned()
            .ok_or_else(|| eyre::eyre!("Unable to resolve relay node {value} from the value"))
    }
}
