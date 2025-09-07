use std::path::Path;

use crate::{
    config::{NodeIdentifier, RelayNode},
    netfs::{self, Command, NetFs, any_fs::AnyFs},
};
use eyre::Context;

#[derive(Clone, Debug)]
pub struct ShareNode {
    pub relay: RelayNode,
}

impl ShareNode {
    pub async fn sync(&self, volume_name: &str, identifer: &NodeIdentifier) -> eyre::Result<()> {
        let client = reqwest::Client::new();
        let response = client
            .get(format!(
                "{}/v1/commands?volume={}&node_id={}",
                self.relay.address, volume_name, identifer.uuid
            ))
            .basic_auth(&self.relay.auth.name, self.relay.auth.password.clone())
            .send()
            .await?;

        if !response.status().is_success() {
            eyre::bail!(
                "Remote answered status {}: {:?}",
                response.status(),
                response.text().await
            )
        }

        let commands = response
            .json::<Vec<Command>>()
            .await
            .wrap_err_with(|| format!("Parsing remote response from {}", self.relay.address))?;

        // TODO:
        // store the current command list
        // command.store()
        // TODO:
        // normalize commands

        // self.apply_commands(commands, fs)

        Ok(())
    }

    pub async fn download(&self, volume_name: &str, path: &Path) -> eyre::Result<Vec<u8>> {
        let client = reqwest::Client::new();
        let response = client
            .get(format!(
                "{}/v1/download?volume={}&path={}",
                self.relay.address,
                volume_name,
                path.display(), // TODO: normalize
            ))
            .basic_auth(&self.relay.auth.name, self.relay.auth.password.clone())
            .send()
            .await?;

        if !response.status().is_success() {
            eyre::bail!(
                "Download failed, remote answered status {}: {:?}",
                response.status(),
                response.text().await
            )
        }

        Ok(response.bytes().await?.to_vec())
    }

    pub async fn apply_commands(&self, commands: &[Command], fs: &AnyFs) -> eyre::Result<()> {
        for command in commands {
            tracing::warn!("Applying {}", command.to_string());

            // TODO:
            // Volume should be implicit
            // * e.g. volume A, at a\\b\\c => /A/a/b/c
            // Pass only AbsNormPath around
            //
            // This is so that we don't have to pass the volume
            // By resolution if a volume is not present then NOENT
            //
            // AbsNormPath::from_path(a: AsRef<Path>, root: Option<AsRef<Path>>)
            // path.component() should work fine
            // => absolute normalized path
            // => we store [String, String, ...]
            // => When loading from string "/a/b/c" we must escape / (linux)
            match command {
                Command::Delete { file } => fs.delete(file).await?,
                Command::Write { file } => {
                    // self.download(volume_name, path);
                    // download file first
                    // fs.write(file, bytes)
                }
                Command::Touch { file } => {
                    fs.delete(file).await?;
                    // download file first
                    // fs.write(file, bytes)
                }
            }
        }

        Ok(())
    }
}
