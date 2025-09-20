use crate::{
    config::{NodeConfig, NodeIdentifier},
    nullfs::{
        any_fs::AnyFs,
        share::{CommandStash, ShareNode},
    },
};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use rand::seq::SliceRandom;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    fmt::{self, Debug},
    hash::Hash,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

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
    pub path: NullFsPath,
    pub file_type: FileType,
    pub stat: FileStat,
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NodeKind {
    File { size: u64 },
    Dir,
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct FileStat {
    pub node: NodeKind,
    pub modified: u64,
    pub created: Option<u64>,
    pub accessed: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Command {
    Delete { file: File },
    Write { file: File },
    Touch { file: File },
    // Rename { from: PathBuf, to: PathBuf },
}

#[derive(Clone, Debug)]
pub struct StashedCommand {
    pub id: String,
    pub hash: String,
    pub command: Command,
    pub timestamp: DateTime<Utc>,
    pub volume: String,
    #[allow(unused)]
    pub state: i32,
}

#[derive(Clone, Debug)]
pub struct Synchronizer;

impl FileType {
    pub fn infer_from_path(path: &NullFsPath) -> Self {
        match path.extension().map(|s| s.to_lowercase()) {
            Some(ext) => match ext.to_lowercase().as_ref() {
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tiff" => FileType::Image,
                "mp4" | "mkv" | "avi" | "mov" | "flv" | "wmv" | "webm" => FileType::Video,
                "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => FileType::Document,
                "exe" | "bat" | "sh" | "bin" | "app" => FileType::Executable,
                "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" => FileType::Archive,
                "txt" | "md" | "csv" | "json" | "xml" | "yaml" | "yml" => FileType::Text,
                _ => FileType::Unkown,
            },
            None => FileType::Unkown,
        }
    }

    pub fn mime_from_path(path: &NullFsPath) -> String {
        path.extension()
            .map(|ext| match ext.to_lowercase().as_ref() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "bmp" => "image/bmp",
                "webp" => "image/webp",
                "tiff" => "image/tiff",
                "mp4" => "video/mp4",
                "mkv" => "video/x-matroska",
                "avi" => "video/x-msvideo",
                "mov" => "video/quicktime",
                "flv" => "video/x-flv",
                "wmv" => "video/x-ms-wmv",
                "webm" => "video/webm",
                "pdf" => "application/pdf",
                "doc" => "application/msword",
                "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "xls" => "application/vnd.ms-excel",
                "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                "ppt" => "application/vnd.ms-powerpoint",
                "pptx" => {
                    "application/vnd.openxmlformats-officedocument.presentationml.presentation"
                }
                "exe" => "application/vnd.microsoft.portable-executable",
                "bat" => "application/x-msdownload",
                "sh" => "application/x-sh",
                "bin" => "application/octet-stream",
                "app" => "application/octet-stream",
                "zip" => "application/zip",
                "rar" => "application/vnd.rar",
                "7z" => "application/x-7z-compressed",
                "tar" => "application/x-tar",
                "gz" => "application/gzip",
                "bz2" => "application/x-bzip2",
                "txt" => "text/plain; charset=utf-8",
                "md" => "text/markdown; charset=utf-8",
                "csv" => "text/csv; charset=utf-8",
                "json" => "application/json",
                "xml" => "application/xml",
                "yaml" | "yml" => "application/x-yaml",
                _ => "application/octet-stream",
            })
            .unwrap_or_else(|| "application/octet-stream")
            .to_owned()
    }
}

impl Synchronizer {
    pub async fn run_sync(
        config: Arc<NodeConfig>,
        identifer: Arc<NodeIdentifier>,
    ) -> eyre::Result<()> {
        tracing::info!("Started sync");
        let tick = tokio::time::Duration::from_secs(config.refresh_secs.unwrap_or(5).max(1));
        let stash_store = CommandStash::new(&identifer).await?;

        let stash = Arc::new(stash_store);
        let mut vol2relay = config
            .volumes
            .clone()
            .into_iter()
            .map(|(volume_name, volume)| {
                volume
                    .pull_from
                    .iter()
                    .map(|share| {
                        config.resolve_alias(share).map(|relay| {
                            (
                                AnyFs::from_volume_item(&volume_name, &volume),
                                ShareNode {
                                    name: share.clone(),
                                    store: stash.clone(),
                                    relay,
                                },
                            )
                        })
                    })
                    .collect::<eyre::Result<Vec<_>>>()
            })
            .collect::<eyre::Result<Vec<_>>>()?;

        if vol2relay.is_empty() {
            eyre::bail!("Resolved no relays in the configuration");
        }

        for (fs, _) in vol2relay.iter_mut().flatten() {
            fs.init().await?;
        }

        loop {
            tracing::info!("{} :: Syncing...", config.name);

            vol2relay.shuffle(&mut rand::rng()); // !

            let identifer = identifer.clone();
            tracing::debug!("Pull/stash state");
            for edge_nodes in vol2relay.iter_mut() {
                edge_nodes.shuffle(&mut rand::rng());

                for (fs, share_node) in edge_nodes {
                    if !share_node.is_alive().await? {
                        continue;
                    }

                    if let Err(e) = share_node.pull(fs, identifer.clone()).await {
                        tracing::error!(
                            "Failed to pull @/{} from {}: {}",
                            fs.get_volume_name(),
                            share_node.name,
                            e
                        );
                    } else {
                        break;
                    }
                }
            }

            tracing::debug!("Apply stashed state");
            for edge_nodes in vol2relay.iter_mut() {
                edge_nodes.shuffle(&mut rand::rng());

                for (fs, share_node) in edge_nodes {
                    if !share_node.is_alive().await? {
                        continue;
                    }

                    if let Err(e) = share_node.apply_commands(fs).await {
                        tracing::error!(
                            "Failed to sync @/{} from {}: {}",
                            fs.get_volume_name(),
                            share_node.name,
                            e
                        );
                    } else {
                        break;
                    }
                }
            }

            tokio::time::sleep(tick).await;
        }
    }

    pub async fn run(
        config: Arc<NodeConfig>,
        identifer: Arc<NodeIdentifier>,
        shutdown: CancellationToken,
    ) -> eyre::Result<()> {
        let task = Self::run_sync(config, identifer);
        tokio::select! {
            _ = task => {},
            _ = shutdown.cancelled() => {}
        };

        Ok(())
    }
}

impl ToString for Command {
    fn to_string(&self) -> String {
        match self {
            Command::Delete { file } => {
                format!("-- {} :: {}", file.path, file.stat.node.to_string())
            }
            Command::Write { file } => {
                format!("++ {} :: {}", file.path, file.stat.node.to_string())
            }
            Command::Touch { file } => format!("?? {}", file.path),
            // Command::Rename { from, to } => format!("** {from} -> {to}"),
        }
    }
}

impl ToString for NodeKind {
    fn to_string(&self) -> String {
        match self {
            NodeKind::File { size } => format!("{size} bytes"),
            NodeKind::Dir => "dir".to_owned(),
        }
    }
}

impl FileStat {
    pub fn is_dir(&self) -> bool {
        matches!(self.node, NodeKind::Dir { .. })
    }

    pub fn is_file(&self) -> bool {
        !self.is_dir()
    }
}

pub fn systime_to_millis(t: SystemTime) -> u64 {
    let time = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    1000 * time.as_secs() + time.subsec_millis() as u64
}

pub fn millis_to_utc(millis: u64) -> DateTime<Utc> {
    let secs = (millis / 1000) as i64;
    let millis = (millis % 1000) as u32;

    Utc.timestamp_opt(secs, millis * 1_000_000) // nanos
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap())
}

#[async_trait]
pub trait NullFs: Debug + Send + Sync {
    async fn init(&mut self) -> eyre::Result<()>;

    async fn dir(&self, dir: &NullFsPath) -> eyre::Result<Vec<File>>;

    async fn mkdir(&self, path: &NullFsPath) -> eyre::Result<()>;

    async fn copy(&self, o: &NullFsPath, d: &NullFsPath) -> eyre::Result<()>;

    async fn rename(&self, o: &NullFsPath, d: &NullFsPath) -> eyre::Result<()>;

    async fn stats(&self, path: &NullFsPath) -> eyre::Result<FileStat>;

    async fn exists(&self, path: &NullFsPath) -> eyre::Result<bool>;

    // TODO: stream
    async fn read(&self, path: &NullFsPath) -> eyre::Result<Vec<u8>>;

    async fn write(&self, file: &File, bytes: &[u8]) -> eyre::Result<()>;

    async fn delete(&self, file: &File) -> eyre::Result<()>;

    /// Computes the hash of a folder entry
    /// * A folder hash is the cumulated hash of its entries
    /// * A file hash is calculated based on its content
    async fn hash(&self, path: &NullFsPath) -> eyre::Result<String>;

    /// Recursively tracks down time based metadata changes
    /// * A folder hash is the cumulated shallow hash of its entries
    /// * A file hash is calculated based on its time of modification
    /// * Cheap way to track down change accross time, especially for modified files
    async fn shallow_hash(&self, file: &File) -> eyre::Result<String>;
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
/// Normalized Posix style only Path implementation
pub struct NullFsPath(Vec<String>);

impl NullFsPath {
    #[allow(unused)]
    pub fn from(path: &Path) -> eyre::Result<Self> {
        Ok(Self(normalize(path)?))
    }

    #[allow(unused)]
    pub fn from_to_str<S: ToString>(s: S) -> eyre::Result<Self> {
        let s = s.to_string();
        let mut ss = s.split('/');
        let first = ss
            .next()
            .ok_or_else(|| eyre::eyre!("Unexpected empty path"))?;
        if !first.starts_with('@') {
            eyre::bail!("Path expected to start with @/");
        }

        Ok(Self(ss.map(|s| s.to_owned()).collect()))
    }

    pub fn volume_name(&self) -> eyre::Result<String> {
        if self.0.is_empty() {
            eyre::bail!("Path is empty");
        }

        Ok(self.0[0].clone())
    }

    #[allow(unused)]
    pub fn extend(&self, comps: Vec<String>) -> eyre::Result<Self> {
        let mut out = self.0.clone();
        out.extend(comps);

        Ok(Self(out))
    }

    pub fn extend_from_rel(&self, path: &Path) -> eyre::Result<Self> {
        let components = path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>();

        self.extend(components)
    }

    #[allow(unused)]
    pub fn components(&self) -> Vec<String> {
        self.0.clone()
    }

    #[allow(unused)]
    pub fn extension(&self) -> Option<String> {
        self.0.last().and_then(|chunk| {
            PathBuf::from(chunk)
                .extension()
                .map(|s| s.to_string_lossy().to_string())
        })
    }
}

impl fmt::Display for NullFsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@/{}", self.0.join("/"))
    }
}

impl Serialize for NullFsPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for NullFsPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error as DeError;

        let s = String::deserialize(deserializer)?;
        NullFsPath::from_to_str(&s).map_err(|e| D::Error::custom(e.to_string()))
    }
}

pub fn normalize(path: &Path) -> eyre::Result<Vec<String>> {
    if path.is_absolute() {
        eyre::bail!("Can only accept relative path");
    }

    let mut new_path = vec![];
    let components = path.components();
    for comp in components {
        new_path.push(comp.as_os_str().to_string_lossy().to_string());
    }

    Ok(new_path)
}

/// Folds contiguous equal subsequence (a variant of RLE algorithm)
/// This is useful for collapsing operations in a noisy log
///
/// E.g. `[1, 2, 3, 1, 2, 3, 4, 5, 4, 5, 1, 2] -> [1, 2, 3, 4, 5, 1, 2]`
pub fn reduce_contiguous_subsequences<T: Eq + Clone>(seq: &[T]) -> Vec<T> {
    let mut out = vec![];
    let mut i = 0;

    while i < seq.len() {
        out.push(seq[i].clone());

        let mut skip = 0;
        for len in (1..=out.len().min(seq.len() - i - 1)).rev() {
            if out[out.len() - len..] == seq[i + 1..i + 1 + len] {
                skip = len; // repeat
                break;
            }
        }

        i += 1 + skip;
    }

    out
}
