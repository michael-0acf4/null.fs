use crate::netfs::{self, File, FileStat, FileType, Filter, NetFs, systime_to_millis};
use async_trait::async_trait;
use eyre::Context;
use path_slash::PathBufExt;
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
    async fn list(&self, search: Option<Filter>) -> eyre::Result<Vec<netfs::File>> {
        let pattern = match search {
            Some(filter) => match filter {
                Filter::Directory { path } => {
                    format!("{}/{}", self.root.join(path).to_slash_lossy(), "*")
                }
                Filter::Glob { pattern } => {
                    format!("{}/{}", self.root.to_slash_lossy(), pattern)
                }
            },
            None => format!("{}/{}", self.root.to_slash_lossy(), "*"),
        };

        let files =
            tokio::task::spawn_blocking(move || -> eyre::Result<Vec<std::path::PathBuf>> {
                let mut paths = vec![];
                for entry in glob::glob(&pattern)? {
                    if let Ok(path) = entry {
                        if path.is_file() {
                            paths.push(path);
                        }
                    }
                }

                Ok(paths)
            })
            .await??;

        let mut result = Vec::with_capacity(files.len());
        for path in files {
            let stat = self.stats(&path).await?;
            result.push(File {
                file_type: FileType::infer_from_path(&path),
                path: path.strip_prefix(&self.root)?.to_path_buf(),
                stat,
            });
        }

        Ok(result)
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
        let metadata = tokio::fs::metadata(path).await?;
        let accessed = metadata.accessed().map(systime_to_millis).ok();
        let modified = metadata.modified().map(systime_to_millis).ok();
        let created = metadata.created().map(systime_to_millis).ok();
        let size = metadata.file_size();

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
        let hash = hex::encode(result);

        Ok(FileStat {
            hash,
            size,
            created,
            accessed,
            modified,
        })
    }
}
