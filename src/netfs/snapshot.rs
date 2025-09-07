use crate::netfs::{self, NetFs, NodeKind};
use async_recursion::async_recursion;
use eyre::{Context, ContextCompat};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf, sync::Arc};

#[derive(Clone, Debug)]
pub struct Snapshot {
    fs: Arc<dyn NetFs>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct State {
    store: IndexMap<PathBuf, netfs::File>,
    dirs: IndexMap<PathBuf, IndexSet<netfs::File>>,
    hashes: IndexMap<PathBuf, String>,
    #[serde(skip)]
    commands: IndexSet<netfs::Command>,
}

impl State {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub async fn update_on_change(
        &mut self,
        file: &netfs::File,
        fs: Arc<dyn NetFs>,
    ) -> eyre::Result<bool> {
        if let Some(prev) = self.store.get(&file.path) {
            if prev.stat.modified != file.stat.modified {
                // For files any change is guaranteed to update mtime
                // For folders only add/del files will update mtime
                // meaning file modification will not be discovered if its inside a folder
                // above the root
                self.store.insert(file.path.clone(), file.clone());

                return Ok(true);
            }

            if file.stat.is_dir() {
                // Detect entries modification (not del/add/rename, handled above)
                let shallow_hash = fs.shallow_hash(file).await?;
                if let Some(prev_hash) = self.hashes.get(&file.path) {
                    if shallow_hash.ne(prev_hash) {
                        self.hashes.insert(file.path.clone(), shallow_hash);

                        return Ok(true);
                    }
                }

                self.hashes.insert(file.path.clone(), shallow_hash);
            }

            return Ok(false);
        }

        self.store.insert(file.path.clone(), file.clone());

        Ok(true)
    }

    pub fn finalize(&mut self) {
        let mut created = HashSet::new();
        for command in &self.commands {
            match command {
                netfs::Command::Delete { file } => {
                    self.store.swap_remove(&file.path);
                    self.dirs.swap_remove(&file.path);
                }
                netfs::Command::Write { file } => {
                    created.insert(file.path.clone());
                }
                _ => {}
            }
        }

        self.commands.retain(|command| {
            if let netfs::Command::Touch { file } = command {
                if created.contains(&file.path) {
                    return false;
                }
            }

            return true;
        });
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
        let is_dir = self.fs.stats(path).await?.is_dir();
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

            if state.update_on_change(&entry, self.fs.clone()).await? {
                if !entry.stat.is_dir() {
                    tracing::warn!("File touched {}", entry.path.display());
                    state.commands.insert(netfs::Command::Touch {
                        file: entry.to_owned(),
                        // the client will have to check the size, if != asks for the hash,
                        // if != then replace the file on their side
                    });
                }

                self.capture_path(state, &entry.path).await?;
            }
        }

        Ok(())
    }
}
