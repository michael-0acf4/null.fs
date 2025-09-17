use crate::{
    config::{StoreKind, VolumeItem},
    nullfs::{self, File, FileStat, NullFs, NullFsPath, local_fs::LocalVolume},
};
use async_trait::async_trait;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct AnyFs {
    pub volume_name: String,
    pub fs_instance: Arc<tokio::sync::Mutex<dyn NullFs>>,
}

impl AnyFs {}

#[async_trait]
impl NullFs for AnyFs {
    async fn init(&mut self) -> eyre::Result<()> {
        let mut fs = self.fs_instance.lock().await;
        fs.init().await
    }

    async fn dir(&self, dir: &NullFsPath) -> eyre::Result<Vec<nullfs::File>> {
        let fs = self.fs_instance.lock().await;
        fs.dir(dir).await
    }

    async fn mkdir(&self, path: &NullFsPath) -> eyre::Result<()> {
        let fs = self.fs_instance.lock().await;
        fs.mkdir(path).await
    }

    async fn copy(&self, o: &NullFsPath, d: &NullFsPath) -> eyre::Result<()> {
        let fs = self.fs_instance.lock().await;
        fs.copy(o, d).await
    }

    async fn rename(&self, o: &NullFsPath, d: &NullFsPath) -> eyre::Result<()> {
        let fs = self.fs_instance.lock().await;
        fs.rename(o, d).await
    }

    async fn stats(&self, path: &NullFsPath) -> eyre::Result<FileStat> {
        let fs = self.fs_instance.lock().await;
        fs.stats(path).await
    }

    async fn hash(&self, path: &NullFsPath) -> eyre::Result<String> {
        let fs = self.fs_instance.lock().await;
        fs.hash(path).await
    }

    async fn exists(&self, path: &NullFsPath) -> eyre::Result<bool> {
        let fs = self.fs_instance.lock().await;
        fs.exists(path).await
    }

    async fn shallow_hash(&self, file: &File) -> eyre::Result<String> {
        let fs = self.fs_instance.lock().await;
        fs.shallow_hash(file).await
    }

    async fn read(&self, path: &NullFsPath) -> eyre::Result<Vec<u8>> {
        let fs = self.fs_instance.lock().await;
        fs.read(path).await
    }

    async fn write(&self, file: &File, bytes: &[u8]) -> eyre::Result<()> {
        let fs = self.fs_instance.lock().await;
        fs.write(file, bytes).await
    }

    async fn delete(&self, file: &File) -> eyre::Result<()> {
        let fs = self.fs_instance.lock().await;
        fs.delete(file).await
    }
}

impl AnyFs {
    pub fn get_volume_name(&self) -> String {
        self.volume_name.clone()
    }

    pub fn volume_root(&self) -> eyre::Result<NullFsPath> {
        NullFsPath::from_to_str(format!("@/{}", self.get_volume_name()))
    }

    pub fn from_volume_item(name: &str, vol: &VolumeItem) -> Self {
        use tokio::sync::Mutex;

        let fs_impl = Arc::new(Mutex::new(match &vol.store {
            StoreKind::Local { root } => LocalVolume {
                name: name.to_owned(),
                root: root.clone(),
            },
        }));

        Self {
            volume_name: name.to_owned(),
            fs_instance: fs_impl as Arc<Mutex<dyn NullFs>>,
        }
    }
}
