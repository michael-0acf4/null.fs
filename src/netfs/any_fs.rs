use std::path::{Path, PathBuf};

use crate::netfs::{self, File, FileStat, NetFs, local_fs::LocalVolume};
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

    fn strip_root_prefix(&self, path: &Path) -> PathBuf {
        match &self {
            AnyFs::Local { expose } => expose.strip_root_prefix(path),
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

    async fn shallow_hash(&self, file: &File) -> eyre::Result<String> {
        match &self {
            AnyFs::Local { expose } => expose.shallow_hash(file).await,
        }
    }

    async fn read(&self, file: &File) -> eyre::Result<Vec<u8>> {
        match &self {
            AnyFs::Local { expose } => expose.read(file).await,
        }
    }

    async fn write(&self, file: &File, bytes: &[u8]) -> eyre::Result<()> {
        match &self {
            AnyFs::Local { expose } => expose.write(file, bytes).await,
        }
    }

    async fn delete(&self, file: &File) -> eyre::Result<()> {
        match &self {
            AnyFs::Local { expose } => expose.delete(file).await,
        }
    }
}

impl AnyFs {
    pub fn get_volume_name(&self) -> String {
        match self {
            AnyFs::Local { expose } => expose.name.to_owned(),
        }
    }

    pub fn get_shares(&self) -> Vec<String> {
        match self {
            AnyFs::Local { expose } => expose.shares.clone(),
        }
    }
}
