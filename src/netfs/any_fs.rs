use crate::netfs::{self, File, FileStat, NetFs, NetFsPath, local_fs::LocalVolume};
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

    async fn dir(&self, dir: &NetFsPath) -> eyre::Result<Vec<netfs::File>> {
        match &self {
            AnyFs::Local { expose } => expose.dir(dir).await,
        }
    }

    async fn mkdir(&self, path: &NetFsPath) -> eyre::Result<()> {
        match &self {
            AnyFs::Local { expose } => expose.mkdir(path).await,
        }
    }

    async fn copy(&self, o: &NetFsPath, d: &NetFsPath) -> eyre::Result<()> {
        match &self {
            AnyFs::Local { expose } => expose.copy(o, d).await,
        }
    }

    async fn rename(&self, o: &NetFsPath, d: &NetFsPath) -> eyre::Result<()> {
        match &self {
            AnyFs::Local { expose } => expose.rename(o, d).await,
        }
    }

    async fn stats(&self, path: &NetFsPath) -> eyre::Result<FileStat> {
        match &self {
            AnyFs::Local { expose } => expose.stats(path).await,
        }
    }

    async fn hash(&self, path: &NetFsPath) -> eyre::Result<String> {
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

    pub fn volume_root(&self) -> eyre::Result<NetFsPath> {
        match self {
            AnyFs::Local { expose } => NetFsPath::from_to_str(&expose.name),
        }
    }

    pub fn get_shares(&self) -> Vec<String> {
        match self {
            AnyFs::Local { expose } => expose.shares.clone(),
        }
    }
}
