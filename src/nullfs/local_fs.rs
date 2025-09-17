use crate::nullfs::{
    self, File, FileStat, FileType, NodeKind, NullFs, NullFsPath, systime_to_millis,
};
use async_trait::async_trait;
use eyre::{Context, ContextCompat};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalVolume {
    pub name: String,
    pub root: PathBuf,
}

impl LocalVolume {
    /// `@/vol_name/b/c` =>` C:/some/root/b/c`
    fn resolve(&self, path: &NullFsPath) -> eyre::Result<PathBuf> {
        let mut components = path.components().into_iter();

        if let Some(comp) = components.next() {
            if comp.ne(&self.name) {
                eyre::bail!(
                    "Wrong volume: first component is expected to be @/{}, got @/{} instead",
                    self.name,
                    comp
                );
            }
        }

        let mut output = PathBuf::new();
        while let Some(comp) = components.next() {
            output.push(comp);
        }

        self.canonicalize(&output)
    }

    /// * `C:/some/root/b/c` -> `@/vol_name/b/c`
    /// * `b/c` -> `@/vol_name/b/c`
    fn to_virtual(&self, path: &Path) -> eyre::Result<NullFsPath> {
        let path = Self::strip_extended_prefix(path.to_path_buf());
        if path.is_relative() {
            return NullFsPath::from_to_str(format!("@/{}", self.name))?.extend_from_rel(&path);
        }

        match path.strip_prefix(&self.root) {
            Ok(out) => NullFsPath::from_to_str(format!("@/{}", self.name))?.extend_from_rel(out),
            Err(_) => {
                eyre::bail!(
                    "Bad prefix: could not make sense of {}, expected prefix {}",
                    path.display(),
                    self.root.display()
                )
            }
        }
    }

    /// UNC-style prefix
    /// `\\?\D:\a` --> `D:\a`
    fn strip_extended_prefix(p: PathBuf) -> PathBuf {
        let s = p.display().to_string();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            PathBuf::from(stripped)
        } else {
            p
        }
    }

    fn canonicalize(&self, path: &Path) -> eyre::Result<PathBuf> {
        let mut path = path.to_path_buf();
        if path.is_relative() {
            path = self.root.join(path);
        }

        Ok(path)
    }
}

#[async_trait]
impl NullFs for LocalVolume {
    async fn init(&mut self) -> eyre::Result<()> {
        self.name = self.name.trim().to_owned();
        self.root = Self::strip_extended_prefix(self.root.canonicalize()?);
        tracing::debug!("/{} <---> {}", self.name, self.root.display());

        Ok(())
    }

    async fn dir(&self, dir: &NullFsPath) -> eyre::Result<Vec<nullfs::File>> {
        let dir = self.resolve(dir)?;

        if dir.is_file() {
            return Ok(vec![]);
        }

        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .with_context(|| format!("Reading directory {}", dir.display()))?;

        let mut results = vec![];
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            tracing::debug!("{} --> {}", path.display(), self.to_virtual(&path)?);
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

    async fn mkdir(&self, path: &NullFsPath) -> eyre::Result<()> {
        tokio::fs::create_dir_all(self.resolve(path)?)
            .await
            .wrap_err(format!("Creating directory {path}"))?;

        Ok(())
    }

    async fn copy(&self, o: &NullFsPath, d: &NullFsPath) -> eyre::Result<()> {
        tokio::fs::copy(self.resolve(o)?, self.resolve(d)?)
            .await
            .wrap_err(format!("Copy {o} to {d}"))?;

        Ok(())
    }

    async fn rename(&self, o: &NullFsPath, d: &NullFsPath) -> eyre::Result<()> {
        tokio::fs::rename(self.resolve(o)?, self.resolve(d)?)
            .await
            .wrap_err(format!("Copy {o} to {d}"))?;

        Ok(())
    }

    async fn stats(&self, path: &NullFsPath) -> eyre::Result<FileStat> {
        let path = self.resolve(path)?;

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
                    size: metadata.len(),
                }
            },
            created,
            accessed,
            modified,
        })
    }

    async fn hash(&self, path: &NullFsPath) -> eyre::Result<String> {
        let resolved_path = self.resolve(path)?;

        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8 * 1024];
        if resolved_path.is_dir() {
            for entry in self.dir(path).await? {
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

        Ok(format!("{:x}", hasher.finalize()))
    }

    async fn shallow_hash(&self, file: &nullfs::File) -> eyre::Result<String> {
        if self.resolve(&file.path)?.is_relative() {
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

        Ok(format!("{:x}", hasher.finalize()))
    }

    async fn exists(&self, path: &NullFsPath) -> eyre::Result<bool> {
        let path = self.resolve(path)?;

        Ok(path.exists())
    }

    async fn read(&self, path: &NullFsPath) -> eyre::Result<Vec<u8>> {
        let path = self.resolve(path)?;

        tokio::fs::read(&path)
            .await
            .wrap_err_with(|| format!("Reading {}", path.display()))
    }

    async fn write(&self, file: &File, bytes: &[u8]) -> eyre::Result<()> {
        let path = self.resolve(&file.path)?;

        if file.stat.is_dir() {
            tokio::fs::create_dir_all(&path).await
        } else {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            tokio::fs::write(&path, bytes).await
        }
        .wrap_err_with(|| format!("Writing ({:?}) {}", file.stat.node, path.display()))
    }

    async fn delete(&self, file: &File) -> eyre::Result<()> {
        let path = self.resolve(&file.path)?;

        if !path.exists() {
            return Ok(());
        }

        if path.is_dir() {
            tokio::fs::remove_dir_all(&path).await
        } else {
            tokio::fs::remove_file(&path).await
        }
        .wrap_err_with(|| format!("Removing {}", path.display()))
    }
}
