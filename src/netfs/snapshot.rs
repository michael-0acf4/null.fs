use actix_web::dev::ResourcePath;
use async_recursion::async_recursion;
use eyre::Context;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::netfs::{self, BasicIdentifier, Command, NetFs};
use std::{collections::HashSet, path::PathBuf, sync::Arc};

#[derive(Clone, Debug)]
pub struct Snapshot {
    fs: Arc<dyn NetFs>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct State {
    store: IndexMap<PathBuf, netfs::File>,
}

impl State {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn update_on_change(&mut self, file: &netfs::File) -> bool {
        if let Some(prev) = self.store.get(&file.path) {
            if prev.stat.modified != file.stat.modified {
                self.store.insert(file.path.clone(), file.clone());

                return true;
            }

            return false;
        }

        self.store.insert(file.path.clone(), file.clone());

        true
    }

    pub fn infer_commands(&self) -> Vec<Command> {
        // from
        todo!()
    }

    pub async fn load_from(path: &PathBuf, create_if_none: bool) -> eyre::Result<Self> {
        if create_if_none {
            Self::new().save_to(path).await?;
        }

        let content = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("Reading state from {}", path.display()))?;

        serde_json::from_str(&content).map_err(|e| e.into())
    }

    pub async fn save_to(&self, path: &PathBuf) -> eyre::Result<()> {
        let content = serde_json::to_string(self)?;
        tokio::fs::write(path, content)
            .await
            .with_context(|| format!("Save state into {}", path.display()))?;

        Ok(())
    }
}

impl Snapshot {
    pub async fn capture(self, state_path: &PathBuf) -> eyre::Result<Vec<netfs::Command>> {
        let mut state = State::load_from(state_path, true).await?;
        self.capture_path(&mut state, &PathBuf::from(".")).await?;
        state.save_to(state_path).await?;

        Ok(state.infer_commands())
    }

    #[async_recursion]
    async fn capture_path(&self, state: &mut State, path: &PathBuf) -> eyre::Result<()> {
        let entries = self.fs.dir(path).await?;

        for entry in entries {
            if state.update_on_change(&entry) {
                self.capture_path(state, &entry.path).await?;
            }
        }

        Ok(())
    }
}
