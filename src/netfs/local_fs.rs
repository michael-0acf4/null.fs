use crate::netfs::{self, File, FileStat, FileType, NetFs, systime_to_millis};
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
        let mut dir = dir.to_owned();
        if dir.is_relative() {
            dir = self.root.join(dir);
        }

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
        let mut path = path.to_owned();
        if path.is_relative() {
            path = self.root.join(path);
        }

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
        let size = metadata.file_size();

        Ok(FileStat {
            is_dir,
            size,
            created,
            accessed,
            modified,
        })
    }

    async fn hash(&self, path: &Path) -> eyre::Result<String> {
        let mut path = path.to_owned();
        if path.is_relative() {
            path = self.root.join(path);
        }

        let file = tokio::fs::File::open(path).await?;
        let mut reader = tokio::io::BufReader::new(file);
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8 * 1024];

        while let Ok(n) = reader.read(&mut buffer).await {
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        let result = hasher.finalize();
        Ok(hex::encode(result))
    }
}
