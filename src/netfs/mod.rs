use crate::{config::NodeConfig, netfs::share::ShareNode};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub mod any_fs;
pub mod local_fs;
pub mod share;

#[derive(Serialize, Deserialize, Clone, Debug)]
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct File {
    pub path: PathBuf,
    pub file_type: FileType,
    pub stat: FileStat,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileStat {
    pub hash: String,
    pub size: u64,
    pub created: Option<u64>,
    pub accessed: Option<u64>,
    pub modified: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum Command {
    Delete { file: File },
    Write { file: File },
    Rename { from: PathBuf, to: PathBuf },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum Filter {
    Directory { path: PathBuf },
    Glob { pattern: String },
}

#[derive(Clone, Debug)]
pub struct Syncrhonizer;

#[async_trait]
pub trait NetFs {
    async fn list(&self, search: Option<Filter>) -> eyre::Result<Vec<File>>;

    async fn mkdir(&self, path: &Path) -> eyre::Result<()>;

    async fn copy(&self, o: &Path, d: &Path) -> eyre::Result<()>;

    async fn rename(&self, o: &Path, d: &Path) -> eyre::Result<()>;

    async fn stats(&self, path: &Path) -> eyre::Result<FileStat>;

    /// Sync accross all shares
    async fn sync(&self, shares: &[ShareNode]) -> eyre::Result<()> {
        for share in shares {
            match share.list(None).await {
                Ok(dest) => {
                    let source = self.list(None).await?;
                    let commands = Command::infer_from(&source, &dest)?;
                    share.send_commands(&commands).await?;
                }
                Err(e) => {}
            }

            // ops = diff(share.get_state(), self.get_state()) -> List<Operation>
            // share.send_commands(ops)
        }

        Ok(())
    }
}

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

impl Command {
    /// Proposes a list of command to go from source to dest
    pub fn infer_from(source: &[File], dest: &[File]) -> eyre::Result<Vec<Command>> {
        #[derive(Clone, Copy, Debug, PartialEq)]
        enum Side {
            Left,
            Right,
        }

        let mut bins = HashMap::new();
        for file in source {
            bins.insert(&file.stat.hash, vec![(Side::Left, file)]);
        }

        for file in dest {
            let item = (Side::Right, file);
            bins.entry(&file.stat.hash)
                .and_modify(|known| {
                    known.push(item);
                })
                .or_insert_with(|| vec![item]);
        }

        let mut commands = vec![];
        for items in bins.values_mut() {
            let count = items.len();
            match count {
                1 => {
                    // only one side has it, other must sync
                    // if true, this node has it => tell the other node
                    // if false, do nothing => the other node will tell us
                    if items[0].0 == Side::Left {
                        commands.push(Command::Write {
                            file: items[0].1.clone(),
                        });
                    }
                    // Deletion cannot be expressed here
                    // I suppose it has to be compared against a persistent record
                    // the node knows what has been deleted (by making a hash enumeration of a snapshot of itself)

                    // I DONT THINK DIFF ON THE FLY WILL DO IT
                    // WE NEED A DATABASE BACKUP
                    // FILE LISTING WILL ACTUALLY PEEK INTO THAT
                    // - fs listing update will be done occasionaly ONLY on entries that has changed timestamps
                    // - By 'watching' we can simply produce a log of operations
                    // - We share the logs instead of the listing
                    // - two nodes will cooperate on a consensus from that log upon refresh
                    // - the consensus will tell the side effects on each side
                    // How does the DB know that it's out of date?
                    // Nah i think we can only tell by the timestamp => we list at boot... any meta changes?
                    // so yeah might as well fail if we cannot rely on that
                    // Also hash will be computed on the fly IMO and only when required
                    // A COMMON TIME FOR AGREEMENT
                    // WHEN WE WRITE WE MUST ALSO INCORPORATE THE METADATA
                }
                2.. => {
                    // same paths
                    // let mut seen: HashMap<&PathBuf, Vec<_>> = HashMap::new();
                    // for item in items {
                    //     if let Some(known) = seen.get_mut(&item.1.path) {
                    //         known.push(item);
                    //     } else {
                    //         seen.insert(&item.1.path, vec![item]);
                    //     }
                    // }

                    todo!("not gonna cut, see above")
                }
                0 => unreachable!(),
            }
        }

        Ok(commands)
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

pub fn systime_to_millis(t: SystemTime) -> u64 {
    let time = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    1000 * time.as_secs() + time.subsec_millis() as u64
}
