use crate::netfs::{self, NetFs};
use async_recursion::async_recursion;
use eyre::{Context, ContextCompat};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Debug)]
pub struct Snapshot {
    fs: Arc<dyn NetFs>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct State {
    store: IndexMap<PathBuf, netfs::File>,
    dirs: IndexMap<PathBuf, IndexSet<netfs::File>>,
    #[serde(skip)]
    commands: IndexSet<netfs::Command>,
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

    pub fn finalize(&mut self) {
        for command in &self.commands {
            match command {
                netfs::Command::Delete { file } => {
                    self.store.swap_remove(&file.path);
                    self.dirs.swap_remove(&file.path);
                }
                _ => {}
            }
        }
    }

    pub fn infer_commands(&self) -> Vec<netfs::Command> {
        // from
        self.commands.clone().into_iter().collect()
    }

    pub async fn load_from(path: &PathBuf, create_if_none: bool) -> eyre::Result<Self> {
        if create_if_none && !path.exists() {
            tracing::warn!("Creating state file {}", path.display());
            Self::new().save_to(path).await?;
        }

        let content = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("Reading state from {}", path.display()))?;

        serde_json::from_str(&content).map_err(|e| e.into())
    }

    pub async fn save_to(&self, path: &PathBuf) -> eyre::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        tokio::fs::write(path, content)
            .await
            .with_context(|| format!("Save state into {}", path.display()))?;

        Ok(())
    }
}

impl Snapshot {
    pub fn new(fs: Arc<dyn NetFs>) -> Self {
        Self { fs }
    }

    pub async fn capture(self, state_path: &PathBuf) -> eyre::Result<Vec<netfs::Command>> {
        let mut state = State::load_from(state_path, true).await?;
        let root = self.fs.get_root_prefix().await?;
        self.capture_path(&mut state, &root).await?;

        state.finalize();
        state.save_to(state_path).await?;

        Ok(state.infer_commands())
    }

    #[async_recursion]
    async fn capture_path(&self, state: &mut State, path: &PathBuf) -> eyre::Result<()> {
        let is_dir = self.fs.stats(path).await?.is_dir;
        if !is_dir {
            return Ok(());
        }

        let mut curr_files = IndexSet::from_iter(self.fs.dir(path).await?.into_iter());
        curr_files.sort_by_key(|k| k.path.clone());
        let prev_files = state.dirs.get(path);

        let mut all_new = false;
        if let Some(prev_files) = prev_files {
            let prev_map = prev_files
                .iter()
                .map(|f| (&f.path, f))
                .collect::<IndexMap<_, _>>();
            let curr_map = curr_files
                .iter()
                .map(|f| (&f.path, f))
                .collect::<IndexMap<_, _>>();

            let prev_set = prev_map.keys().collect::<IndexSet<_>>();
            let curr_set = curr_map.keys().collect::<IndexSet<_>>();

            let added = curr_set.difference(&prev_set);
            let removed = prev_set.difference(&curr_set);

            for item in added {
                let item = curr_map.get(*item).wrap_err_with(|| {
                    format!("Fatal: expected item to be found in current history")
                })?;

                state.commands.insert(netfs::Command::Write {
                    file: (*item).to_owned(),
                });
            }

            for item in removed {
                let item = prev_map.get(*item).wrap_err_with(|| {
                    format!("Fatal: expected item to be found in previous history")
                })?;

                state.commands.insert(netfs::Command::Delete {
                    file: (*item).to_owned(),
                });
            }
        } else {
            all_new = true;
        }

        state.dirs.insert(path.to_owned(), curr_files.clone());

        for entry in curr_files {
            if all_new {
                state.commands.insert(netfs::Command::Write {
                    file: entry.to_owned(),
                });
            }

            if state.update_on_change(&entry) {
                self.capture_path(state, &entry.path).await?;
            }
        }

        Ok(())
    }
}
