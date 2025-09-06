use std::path::{Path, PathBuf};

use crate::netfs::{self, FileStat, NetFs, local_fs::LocalVolume};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum AnyFs {
    Local { expose: LocalVolume },
    // TODO: s3
}

#[async_trait]
impl NetFs for AnyFs {
    async fn init(&mut self) -> eyre::Result<()> {
        match self {
            AnyFs::Local { expose } => expose.init().await,
        }
    }

    async fn get_root_prefix(&self) -> eyre::Result<PathBuf> {
        match &self {
            AnyFs::Local { expose } => expose.get_root_prefix().await,
        }
    }

    async fn dir(&self, dir: &PathBuf) -> eyre::Result<Vec<netfs::File>> {
        match &self {
            AnyFs::Local { expose } => expose.dir(dir).await,
        }
    }

    async fn mkdir(&self, path: &Path) -> eyre::Result<()> {
        match &self {
            AnyFs::Local { expose } => expose.mkdir(path).await,
        }
    }

    async fn copy(&self, o: &Path, d: &Path) -> eyre::Result<()> {
        match &self {
            AnyFs::Local { expose } => expose.copy(o, d).await,
        }
    }

    async fn rename(&self, o: &Path, d: &Path) -> eyre::Result<()> {
        match &self {
            AnyFs::Local { expose } => expose.rename(o, d).await,
        }
    }

    async fn stats(&self, path: &Path) -> eyre::Result<FileStat> {
        match &self {
            AnyFs::Local { expose } => expose.stats(path).await,
        }
    }

    async fn hash(&self, path: &Path) -> eyre::Result<String> {
        match &self {
            AnyFs::Local { expose } => expose.hash(path).await,
        }
    }
}

impl AnyFs {
    pub fn get_volume_name(&self) -> String {
        match self {
            AnyFs::Local { expose } => expose.name.to_owned(),
        }
    }
}
