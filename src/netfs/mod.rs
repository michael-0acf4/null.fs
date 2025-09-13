use crate::{
    config::{NodeConfig, NodeIdentifier, RelayNode},
    netfs::share::ShareNode,
};
use async_trait::async_trait;
use color_eyre::Section;
use eyre::Context;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    fmt::{self, Debug},
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
    pub path: NetFsPath,
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
pub struct Syncrhonizer;

impl FileType {
    pub fn infer_from_path(path: &NetFsPath) -> Self {
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
}

impl Syncrhonizer {
    pub async fn run(config: &NodeConfig, identifer: &NodeIdentifier) -> eyre::Result<()> {
        let tick = tokio::time::Duration::from_secs(5);
        let relays = config
            .volumes
            .iter()
            .map(|volume| {
                volume
                    .get_shares()
                    .iter()
                    .map(|share| {
                        config
                            .resolve_alias(&share)
                            .map(|relay| (volume.get_volume_name(), ShareNode { relay }))
                    })
                    .collect::<Vec<_>>()
            })
            .flatten()
            .collect::<eyre::Result<Vec<_>>>()?;

        loop {
            tracing::info!("Sync");
            for (volume_name, share_node) in &relays {
                if let Err(e) = share_node.sync(volume_name, identifer).await {
                    tracing::error!("Failed to sync @/{volume_name}: {e}");
                }
            }
            tokio::time::sleep(tick).await;
        }
    }
}

impl RelayNode {
    pub async fn sync(&self) -> eyre::Result<()> {
        Ok(())
    }
}

impl ToString for Command {
    fn to_string(&self) -> String {
        match self {
            Command::Delete { file } => {
                format!("-- {} :: {:?}", file.path, file.stat.node)
            }
            Command::Write { file } => {
                format!("++ {} :: {:?}", file.path, file.stat.node)
            }
            Command::Touch { file } => format!("?? {}", file.path),
            // Command::Rename { from, to } => format!("** {from} -> {to}"),
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

#[async_trait]
pub trait NetFs: Debug + Send + Sync {
    async fn init(&mut self) -> eyre::Result<()>;

    async fn dir(&self, dir: &NetFsPath) -> eyre::Result<Vec<File>>;

    async fn mkdir(&self, path: &NetFsPath) -> eyre::Result<()>;

    async fn copy(&self, o: &NetFsPath, d: &NetFsPath) -> eyre::Result<()>;

    async fn rename(&self, o: &NetFsPath, d: &NetFsPath) -> eyre::Result<()>;

    async fn stats(&self, path: &NetFsPath) -> eyre::Result<FileStat>;

    // FIXME: stream
    async fn read(&self, file: &File) -> eyre::Result<Vec<u8>>;

    async fn write(&self, file: &File, bytes: &[u8]) -> eyre::Result<()>;

    async fn delete(&self, file: &File) -> eyre::Result<()>;

    /// Computes the hash of a folder entry
    /// * A folder hash is the cumulated hash of its entries
    /// * A file hash is calculated based on its content
    async fn hash(&self, path: &NetFsPath) -> eyre::Result<String>;

    /// Recursively tracks down time based metadata changes
    /// * A folder hash is the cumulated shallow hash of its entries
    /// * A file hash is calculated based on its time of modification
    /// * Cheap way to track down change accross time, especially for modified files
    async fn shallow_hash(&self, file: &File) -> eyre::Result<String>;
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
/// Normalized Posix style only Path implementation
pub struct NetFsPath(Vec<String>);

impl NetFsPath {
    pub fn empty() -> Self {
        Self(vec![])
    }

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
            .into_iter()
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
        self.0
            .last()
            .map(|chunk| {
                PathBuf::from(chunk)
                    .extension()
                    .map(|s| s.to_string_lossy().to_string())
            })
            .flatten()
    }
}

impl fmt::Display for NetFsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@/{}", self.0.join("/"))
    }
}

impl Serialize for NetFsPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for NetFsPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error as DeError;

        let s = String::deserialize(deserializer)?;
        NetFsPath::from_to_str(&s).map_err(|e| D::Error::custom(e.to_string()))
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
