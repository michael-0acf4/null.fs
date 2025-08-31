use std::{path::Path, sync::Arc};

use crate::netfs::{self, FileStat, Filter, NetFs, local_fs::LocalVolume};
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
    async fn list(&self, search: Option<Filter>) -> eyre::Result<Vec<netfs::File>> {
        match &self {
            AnyFs::Local { expose } => expose.list(search).await,
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
}

impl AnyFs {
    pub fn get_volume_name(&self) -> String {
        match self {
            AnyFs::Local { expose } => expose.name.to_owned(),
        }
    }
}
