use crate::{
    nullfs::NullFs,
    nullfs::NullFsPath,
    nullfs::any_fs::AnyFs,
    nullfs::{Command, File},
};
use async_recursion::async_recursion;
use eyre::{Context, ContextCompat};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf};

#[derive(Clone, Debug)]
pub struct Snapshot {
    fs: AnyFs,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct State {
    store: IndexMap<NullFsPath, File>,
    dirs: IndexMap<NullFsPath, IndexSet<File>>,
    hashes: IndexMap<NullFsPath, String>,
    #[serde(skip)]
    commands: IndexSet<Command>,
}

impl State {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn update_on_change(&mut self, file: &File) -> eyre::Result<bool> {
        if file.stat.is_dir() {
            eyre::bail!("Fatal: expected entry to be a file");
        }

        if let Some(prev) = self.store.get(&file.path) {
            if prev.stat.modified != file.stat.modified {
                self.store.insert(file.path.clone(), file.clone());

                return Ok(true);
            }

            return Ok(false);
        }

        self.store.insert(file.path.clone(), file.clone());

        Ok(true)
    }

    pub fn finalize(&mut self) {
        let mut created = HashSet::new();
        let commands = self.commands.clone();
        for command in commands {
            match command {
                Command::Delete { file } => {
                    self.store.swap_remove(&file.path);
                    self.dirs.swap_remove(&file.path);
                }
                Command::Write { file } => {
                    created.insert(file.path.clone());
                }
                Command::Touch { .. } => {}
            }
        }

        // False touch
        self.commands.retain(|command| {
            if let Command::Touch { file } = command {
                if created.contains(&file.path) {
                    return false;
                }
            }

            true
        });

        // TODO: rename concept? (deletion + addition where file content matches)
        // Renames require knowing the (path, old hash) and comparing all files
        // Computing the hash for all files is not cheap
        // Can't avoid O(n^2)
    }

    pub fn infer_commands(self) -> Vec<Command> {
        self.commands.into_iter().collect()
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
        tracing::debug!("Saving state {}", path.display());
        let content = serde_json::to_string(self)?;
        tokio::fs::write(path, content)
            .await
            .with_context(|| format!("Save state into {}", path.display()))?;

        Ok(())
    }
}

impl Snapshot {
    pub fn new(fs: AnyFs) -> Self {
        Self { fs }
    }

    pub async fn capture(self, state_path: &PathBuf) -> eyre::Result<Vec<Command>> {
        let mut state = State::load_from(state_path, true).await?;
        let root = self.fs.volume_root()?;
        self.capture_path(&mut state, &root).await?;

        state.finalize();
        state.save_to(state_path).await?;

        Ok(state.infer_commands())
    }

    #[async_recursion]
    async fn capture_path(&self, state: &mut State, path: &NullFsPath) -> eyre::Result<()> {
        let is_dir = self.fs.stats(path).await?.is_dir();
        if !is_dir {
            return Ok(());
        }

        let mut curr_files = IndexSet::from_iter(self.fs.dir(path).await?.into_iter());
        curr_files.sort_by_key(|k| k.path.to_string());
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
                    "Fatal: expected item to be found in current history".to_string()
                })?;

                state.commands.insert(Command::Write {
                    file: (*item).to_owned(),
                });
            }

            for item in removed {
                let item = prev_map.get(*item).wrap_err_with(|| {
                    "Fatal: expected item to be found in previous history".to_string()
                })?;

                state.commands.insert(Command::Delete {
                    file: (*item).to_owned(),
                });
            }
        } else {
            all_new = true;
        }

        state.dirs.insert(path.to_owned(), curr_files.clone());

        for entry in curr_files {
            if all_new {
                state.commands.insert(Command::Write {
                    file: entry.to_owned(),
                });
            }

            if entry.stat.is_file() {
                if state.update_on_change(&entry)? {
                    state.commands.insert(Command::Touch {
                        file: entry.to_owned(),
                        // the client will have to check the size, if != asks for the hash,
                        // if != then replace the file on their side
                    });
                }
            } else {
                self.capture_path(state, &entry.path).await?;
            }
        }

        Ok(())
    }
}
