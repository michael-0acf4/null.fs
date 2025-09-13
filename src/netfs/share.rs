use std::path::Path;

use crate::{
    config::{NodeIdentifier, RelayNode},
    netfs::{Command, NetFs, NetFsPath, any_fs::AnyFs},
};
use eyre::Context;

#[derive(Clone, Debug)]
pub struct ShareNode {
    pub relay: RelayNode,
}

impl ShareNode {
    pub async fn sync(&self, fs: &AnyFs, identifer: &NodeIdentifier) -> eyre::Result<()> {
        let client = reqwest::Client::new();
        let response = client
            .get(format!(
                "{}/v1/commands?volume={}&node_id={}",
                self.relay.address,
                fs.get_volume_name(),
                identifer.uuid
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

        let external_changes = response
            .json::<Vec<Command>>()
            .await
            .wrap_err_with(|| format!("Parsing remote response from {}", self.relay.address))?;

        // TODO:
        // store the current command list
        // * need a way to mark commands that are not or are done (survive restart)
        // command.store()
        // TODO:
        // normalize commands

        self.apply_commands(&external_changes, &fs).await?;

        Ok(())
    }

    pub async fn download(&self, path: &NetFsPath) -> eyre::Result<Vec<u8>> {
        let client = reqwest::Client::new();
        let response = client
            .get(format!(
                "{}/v1/download?path={}",
                self.relay.address,
                path.to_string(),
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

    pub async fn ask_for_hash(&self, path: &NetFsPath) -> eyre::Result<String> {
        let client = reqwest::Client::new();
        let response = client
            .get(format!(
                "{}/v1/hash?path={}",
                self.relay.address,
                path.to_string(),
            ))
            .basic_auth(&self.relay.auth.name, self.relay.auth.password.clone())
            .send()
            .await?;

        if !response.status().is_success() {
            eyre::bail!(
                "Could not get hash, remote answered status {}: {:?}",
                response.status(),
                response.text().await
            )
        }

        response.json().await.map_err(|e| e.into())
    }

    pub async fn apply_commands(&self, commands: &[Command], fs: &AnyFs) -> eyre::Result<()> {
        for command in commands {
            tracing::warn!("Applying {}", command.to_string());

            match command {
                Command::Delete { file } => fs.delete(file).await?,
                Command::Write { file } => {
                    if fs.exists(&file.path).await? {
                        let remote_hash = self.ask_for_hash(&file.path).await?;
                        let local_hash = fs.hash(&file.path).await?;
                        if remote_hash == local_hash {
                            continue;
                        }
                    }

                    let data = self.download(&file.path).await?;
                    fs.write(file, &data).await?;
                }
                Command::Touch { file } => {
                    if fs.exists(&file.path).await? {
                        let remote_hash = self.ask_for_hash(&file.path).await?;
                        let local_hash = fs.hash(&file.path).await?;
                        if remote_hash == local_hash {
                            continue;
                        }
                    }

                    fs.delete(file).await?;
                    let data = self.download(&file.path).await?;
                    fs.write(file, &data).await?;
                }
            }
        }

        Ok(())
    }
}
