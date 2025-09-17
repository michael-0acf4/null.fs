use crate::nullfs::{NullFs, any_fs::AnyFs};
use eyre::{Context, ContextCompat};
use indexmap::{IndexMap, IndexSet};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub name: String,
    pub password: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RelayNode {
    pub address: Url,
    pub auth: User,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum StoreKind {
    Local { root: PathBuf },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VolumeItem {
    pub allow: Vec<String>,
    pub pull_from: Vec<String>,
    pub store: StoreKind,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NodeConfig {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub refresh_secs: Option<u64>,
    pub users: IndexSet<User>,
    pub relay_nodes: IndexMap<String, RelayNode>,
    pub volumes: IndexMap<String, VolumeItem>,
}

impl NodeConfig {
    pub async fn load_from_file(path: &Path) -> eyre::Result<Self> {
        let content = tokio::fs::read_to_string(path)
            .await
            .wrap_err_with(|| format!("Loading configuration file at {}", path.display()))?;

        let config = serde_yaml::from_str::<Self>(&content)
            .wrap_err_with(|| "Parsing configuration file".to_string())?;

        config.validate()
    }

    fn validate(self) -> eyre::Result<Self> {
        for relay in self.relay_nodes.values() {
            if let Some(port) = relay.address.port() {
                let host = relay
                    .address
                    .host()
                    .ok_or_else(|| format!("No host: {}", relay.address))
                    .map_err(|e| eyre::eyre!(e))?
                    .to_string();
                let same_host = host.eq("0.0.0.0") || host.eq("127.0.0.1") || host.eq("localhost");

                if port == self.port && same_host {
                    eyre::bail!(
                        "Relay node {} is pointing to the current node",
                        relay.address
                    );
                }
            }
        }

        let mut seen = HashSet::new();
        let mut duplicates = HashSet::new();
        for user in &self.users {
            if seen.contains(&user.name) {
                duplicates.insert(user.name.clone());
            } else {
                seen.insert(user.name.clone());
            }
        }

        if !duplicates.is_empty() {
            eyre::bail!(
                "User(s) have duplicates: {}",
                duplicates.into_iter().collect::<Vec<_>>().join(", ")
            );
        }

        for (_, vol) in &self.volumes {
            for uname in &vol.allow {
                if self.resolve_user(uname).is_none() {
                    eyre::bail!(
                        "User {:?} is not defined, expected: {}",
                        uname,
                        self.users
                            .iter()
                            .map(|user| format!("{:?}", user.name))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
        }

        Ok(self)
    }

    pub fn resolve_alias(&self, value: &str) -> eyre::Result<RelayNode> {
        self.relay_nodes
            .get(value)
            .cloned()
            .ok_or_else(|| eyre::eyre!("Unable to resolve relay node {value} from the value"))
    }

    pub fn resolve_user(&self, name: &str) -> Option<&User> {
        self.users.iter().find(|user| user.name.eq(name))
    }

    pub fn allow(&self, volume: &str, user: User) -> bool {
        if let Some(vol) = self.volumes.get(volume) {
            for uname in &vol.allow {
                let known_user = self.resolve_user(uname).with_context(|| {
                    format!(
                        "Expected to know user {uname:?}: invalid config passed through validation"
                    )
                }).unwrap();

                if known_user.eq(&user) {
                    return true;
                }
            }
        }

        false
    }

    pub async fn get_initialized_fs_volume(
        &self,
        volume_name: &str,
    ) -> eyre::Result<Option<AnyFs>> {
        if let Some(volume) = self.volumes.get(volume_name) {
            let mut fs = AnyFs::from_volume_item(volume_name, volume);
            fs.init().await?;
            return Ok(Some(fs));
        }

        Ok(None)
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
