use crate::nullfs::any_fs::AnyFs;
use eyre::Context;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
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

    pub fn find_volume(&self, volume: &str) -> Option<&AnyFs> {
        self.volumes
            .iter()
            .find(|fs| fs.get_volume_name().eq(volume))
    }

    pub fn allow(&self, volume: &str, user: User) -> eyre::Result<bool> {
        if let Some(fs) = self.find_volume(volume) {
            let res_shares = fs
                .get_shares()
                .iter()
                .map(|share| self.resolve_alias(&share))
                .collect::<eyre::Result<Vec<_>>>()?;

            return Ok(res_shares.iter().find(|relay| relay.auth == user).is_some());
        }

        Ok(false)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NodeIdentifier {
    pub uuid: String,
}

impl NodeIdentifier {
    pub fn load_from_file(path: &Path) -> eyre::Result<Self> {
        if path.exists() {
            return std::fs::read_to_string(path)
                .and_then(|content| serde_json::from_str::<Self>(&content).map_err(|e| e.into()))
                .wrap_err_with(|| format!("Reading id from {}", path.display()));
        }

        let new_one = Self {
            uuid: Uuid::new_v4().to_string(),
        };

        tracing::warn!("File id not found, generating a new one");
        std::fs::write(path, serde_json::to_string(&new_one).unwrap())
            .wrap_err_with(|| format!("Writing id to {}", path.display()))?;

        Ok(new_one)
    }
}
