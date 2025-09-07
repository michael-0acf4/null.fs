use crate::netfs::{self, File, FileStat, FileType, NetFs, NodeKind, systime_to_millis};
use async_trait::async_trait;
use eyre::{Context, ContextCompat};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
};
use tokio::io::AsyncReadExt;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalVolume {
    pub name: String,
    pub root: PathBuf,
    pub shares: Vec<String>,
}

impl LocalVolume {
    fn canonicalize(&self, path: &PathBuf) -> PathBuf {
        let mut path = path.clone();
        if path.is_relative() {
            path = self.root.join(path);
        }

        path
    }
}

#[async_trait]
impl NetFs for LocalVolume {
    async fn init(&mut self) -> eyre::Result<()> {
        self.name = self.name.trim().to_owned();
        self.root = tokio::fs::canonicalize(&self.root).await?;

        Ok(())
    }

    async fn get_root_prefix(&self) -> eyre::Result<PathBuf> {
        Ok(self.root.clone())
    }

    async fn dir(&self, dir: &PathBuf) -> eyre::Result<Vec<netfs::File>> {
        let dir = self.canonicalize(&dir);

        if dir.is_file() {
            return Ok(vec![]);
        }

        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .with_context(|| format!("Reading directory {}", dir.display()))?;

        let mut results = vec![];
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let stat = self.stats(&path).await?;
            let file_type = FileType::infer_from_path(&path);

            results.push(File {
                path,
                file_type,
                stat,
            });
        }

        Ok(results)
    }

    async fn mkdir(&self, path: &Path) -> eyre::Result<()> {
        tokio::fs::create_dir_all(path)
            .await
            .wrap_err(format!("Creating directory {}", path.display()))?;

        Ok(())
    }

    async fn copy(&self, o: &Path, d: &Path) -> eyre::Result<()> {
        tokio::fs::copy(o, d)
            .await
            .wrap_err(format!("Copy {} to {}", o.display(), d.display()))?;

        Ok(())
    }

    async fn rename(&self, o: &Path, d: &Path) -> eyre::Result<()> {
        tokio::fs::rename(o, d).await.wrap_err(format!(
            "Copy {} to {}",
            o.display(),
            d.display()
        ))?;

        Ok(())
    }

    async fn stats(&self, path: &Path) -> eyre::Result<FileStat> {
        let path = self.canonicalize(&path.to_path_buf());

        let metadata = tokio::fs::metadata(&path)
            .await
            .with_context(|| format!("Could not read metadata for {}", path.display()))?;
        let accessed = metadata.accessed().map(systime_to_millis).ok();
        let modified = metadata
            .modified()
            .map(systime_to_millis)
            .ok()
            .with_context(|| format!("Could not read modified time for {}", path.display()))?;
        let created = metadata.created().map(systime_to_millis).ok();
        let is_dir = metadata.is_dir();

        Ok(FileStat {
            node: if is_dir {
                NodeKind::Dir
            } else {
                NodeKind::File {
                    size: metadata.file_size(),
                }
            },
            created,
            accessed,
            modified,
        })
    }

    async fn hash(&self, path: &Path) -> eyre::Result<String> {
        let path = self.canonicalize(&path.to_path_buf());

        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8 * 1024];
        if path.is_dir() {
            for entry in self.dir(&path).await? {
                let hash = self.hash(&entry.path).await?;
                hasher.update(entry.path.display().to_string());
                hasher.update(hash);
            }
        } else {
            let file = tokio::fs::File::open(path).await?;
            let mut reader = tokio::io::BufReader::new(file);

            while let Ok(n) = reader.read(&mut buffer).await {
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
        }

        let result = hasher.finalize();
        Ok(hex::encode(result))
    }

    async fn shallow_hash(&self, file: &netfs::File) -> eyre::Result<String> {
        if file.path.is_relative() {
            eyre::bail!("Provided file has a relative path {}", file.path.display());
        }

        let mut hasher = Sha256::new();
        hasher.update(file.stat.modified.to_string());

        match file.stat.node {
            NodeKind::Dir => {
                for entry in self.dir(&file.path).await? {
                    let hash = self.shallow_hash(&entry).await?;
                    hasher.update(hash);
                }
            }
            NodeKind::File { size } => {
                hasher.update(size.to_string());
            }
        }

        let result = hasher.finalize();
        Ok(hex::encode(result))
    }
}
