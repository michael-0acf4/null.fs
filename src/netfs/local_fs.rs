use crate::netfs::{self, File, FileStat, FileType, NetFs, NetFsPath, NodeKind, systime_to_millis};
use async_trait::async_trait;
use camino::Utf8PathBuf;
use eyre::{Context, ContextCompat};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{os::windows::fs::MetadataExt, path::PathBuf};
use tokio::io::AsyncReadExt;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalVolume {
    pub name: String,
    pub root: PathBuf,
    pub shares: Vec<String>,
}

// TODO:
// seems to be the good path
// ok so basically
// instead
// all path should be absolute, emphasizing file abstraction
// /@netfs-A/some/file.txt
// For local fs it needs to map /@netfs-A/some/file.txt to local
// NetFsPath::canonicalize() should be removed, itis impossible to resolve
// TOOD:2
// fn resolve(p: NetFsPath, vol) -> path according to volume

impl LocalVolume {
    // move to anyfs?
    /// /A/b/c => C:/some/root/b/c
    fn resolve(&self, path: &NetFsPath) -> eyre::Result<PathBuf> {
        if path.is_relative() {
            return Ok(self.canonicalize(&path.to_host_path()));
        }

        let mut components = path.components().into_iter();
        if let Some(comp) = components.next() {
            if comp.eq(&format!("/{}", self.name)) {}
        }

        let mut output = PathBuf::new();
        while let Some(comp) = components.next() {
            output.push(comp);
        }

        return Ok(self.canonicalize(&output));
    }

    // C:/some/root/b/c -> /A/b/c
    // b/c -> /A/b/c
    fn to_virtual(&self, path: &PathBuf) -> eyre::Result<NetFsPath> {
        if path.is_relative() {
            return NetFsPath::from_to_str(format!("/{}", self.name))?
                .join(&path.display().to_string());
        }

        match path.strip_prefix(&self.root) {
            Ok(out) => {
                NetFsPath::from_to_str(format!("/{}", self.name))?.join(&out.display().to_string())
            }
            Err(_) => {
                eyre::bail!("Bad prefix: could not make sense of {}", path.display())
            }
        }
    }

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
        self.root = self.root.canonicalize()?;

        Ok(())
    }

    async fn dir(&self, dir: &NetFsPath) -> eyre::Result<Vec<netfs::File>> {
        let dir = self.resolve(&dir)?;

        if dir.is_file() {
            return Ok(vec![]);
        }

        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .with_context(|| format!("Reading directory {}", dir.display()))?;

        let mut results = vec![];
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let vpath = self.to_virtual(&path)?;
            let stat = self.stats(&vpath).await?;
            let file_type = FileType::infer_from_path(&vpath);

            results.push(File {
                path: vpath,
                file_type,
                stat,
            });
        }

        Ok(results)
    }

    async fn mkdir(&self, path: &NetFsPath) -> eyre::Result<()> {
        tokio::fs::create_dir_all(self.resolve(path)?)
            .await
            .wrap_err(format!("Creating directory {path}"))?;

        Ok(())
    }

    async fn copy(&self, o: &NetFsPath, d: &NetFsPath) -> eyre::Result<()> {
        tokio::fs::copy(self.resolve(o)?, self.resolve(d)?)
            .await
            .wrap_err(format!("Copy {o} to {d}"))?;

        Ok(())
    }

    async fn rename(&self, o: &NetFsPath, d: &NetFsPath) -> eyre::Result<()> {
        tokio::fs::rename(self.resolve(o)?, self.resolve(d)?)
            .await
            .wrap_err(format!("Copy {o} to {d}"))?;

        Ok(())
    }

    async fn stats(&self, path: &NetFsPath) -> eyre::Result<FileStat> {
        panic!("CONVERT {} ---> {}", path, self.resolve(&path)?.display());
        let path = self.resolve(&path)?;

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

    async fn hash(&self, path: &NetFsPath) -> eyre::Result<String> {
        let resolved_path = self.resolve(&path)?;

        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8 * 1024];
        if resolved_path.is_dir() {
            for entry in self.dir(&path).await? {
                let hash = self.hash(&entry.path).await?;
                hasher.update(entry.path.to_string());
                hasher.update(hash);
            }
        } else {
            let file = tokio::fs::File::open(resolved_path).await?;
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
            eyre::bail!("Provided file has a relative path {}", file.path);
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

    async fn read(&self, file: &File) -> eyre::Result<Vec<u8>> {
        let path = self.resolve(&file.path)?;

        tokio::fs::read(&path)
            .await
            .wrap_err_with(|| format!("Reading {}", path.display()))
    }

    async fn write(&self, file: &File, bytes: &[u8]) -> eyre::Result<()> {
        let path = self.resolve(&file.path)?;

        tokio::fs::write(&path, bytes)
            .await
            .wrap_err_with(|| format!("Writing {}", path.display()))
    }

    async fn delete(&self, file: &File) -> eyre::Result<()> {
        let path = self.resolve(&file.path)?;

        if path.is_dir() {
            tokio::fs::remove_dir_all(&path).await
        } else {
            tokio::fs::remove_file(&path).await
        }
        .wrap_err_with(|| format!("Writing {}", path.display()))
    }
}
