use crate::{config::NodeConfig, netfs::share::ShareNode};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::Debug,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub mod any_fs;
pub mod local_fs;
pub mod share;
pub mod snapshot;

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    Image,
    Video,
    Document,
    Executable,
    Archive,
    Text,
    Unkown,
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct File {
    pub path: PathBuf,
    pub file_type: FileType,
    pub stat: FileStat,
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct FileStat {
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
    pub created: Option<u64>,
    pub accessed: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Command {
    Delete { file: File },
    Write { file: File },
    Rename { from: PathBuf, to: PathBuf },
}

#[derive(Clone, Debug)]
pub struct Syncrhonizer;

impl FileType {
    pub fn infer_from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.to_lowercase())
        {
            Some(ext) => match ext.to_lowercase().as_ref() {
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tiff" => FileType::Image,
                "mp4" | "mkv" | "avi" | "mov" | "flv" | "wmv" => FileType::Video,
                "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => FileType::Document,
                "exe" | "bat" | "sh" | "bin" | "app" => FileType::Executable,
                "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" => FileType::Archive,
                "txt" | "md" | "csv" | "json" | "xml" | "yaml" | "yml" => FileType::Text,
                _ => FileType::Unkown,
            },
            None => FileType::Unkown,
        }
    }
}

impl Syncrhonizer {
    pub async fn run(config: &NodeConfig) -> eyre::Result<()> {
        let tick = tokio::time::Duration::from_secs(5);
        loop {
            tracing::info!("Sync");
            tokio::time::sleep(tick).await;
        }
    }
}

impl ToString for Command {
    fn to_string(&self) -> String {
        match self {
            Command::Delete { file } => {
                format!(
                    "-- {} :: {}",
                    file.path.display(),
                    if file.stat.is_dir { "Dir" } else { "File" }
                )
            }
            Command::Write { file } => {
                format!(
                    "++ {} :: {}",
                    file.path.display(),
                    if file.stat.is_dir { "Dir" } else { "File" }
                )
            }
            Command::Rename { from, to } => format!("** {} -> {}", from.display(), to.display()),
        }
    }
}

pub fn systime_to_millis(t: SystemTime) -> u64 {
    let time = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    1000 * time.as_secs() + time.subsec_millis() as u64
}

#[async_trait]
pub trait NetFs: Debug + Send + Sync {
    async fn init(&mut self) -> eyre::Result<()>;

    async fn get_root_prefix(&self) -> eyre::Result<PathBuf>;

    async fn dir(&self, dir: &PathBuf) -> eyre::Result<Vec<File>>;

    async fn mkdir(&self, path: &Path) -> eyre::Result<()>;

    async fn copy(&self, o: &Path, d: &Path) -> eyre::Result<()>;

    async fn rename(&self, o: &Path, d: &Path) -> eyre::Result<()>;

    async fn stats(&self, path: &Path) -> eyre::Result<FileStat>;

    async fn hash(&self, path: &Path) -> eyre::Result<String>;

    /// Sync accross all shares
    async fn sync(&self, shares: &[ShareNode]) -> eyre::Result<()> {
        Ok(())
    }
}
