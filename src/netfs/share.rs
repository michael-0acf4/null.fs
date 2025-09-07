use std::path::PathBuf;

use crate::netfs::{self, Command};
use eyre::Context;
use reqwest::Url;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ShareNode {
    address: Url,
    user: String,
    password: Option<String>,
}

impl ShareNode {
    pub async fn dir(&self, search: &PathBuf) -> eyre::Result<Vec<netfs::File>> {
        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/v1/dir", self.address))
            .basic_auth(&self.user, self.password.clone())
            .json(&search)
            .send()
            .await?;

        if response.status().is_success() {
            let files = response
                .json::<Vec<_>>()
                .await
                .wrap_err_with(|| format!("Parsing remote response from {}", self.address))?;

            return Ok(files);
        }

        eyre::bail!(
            "Remote answered status {}: {:?}",
            response.status(),
            response.text().await
        )
    }

    pub async fn send_commands(&self, commands: &[Command]) -> eyre::Result<()> {
        let client = reqwest::Client::new();

        for command in commands {
            let response = client
                .post(format!("{}/v1/command", self.address))
                .json(command)
                .send()
                .await?;

            if response.status().is_success() {}

            match command {
                Command::Delete { file } => todo!(),
                Command::Write { file } => todo!(),
                Command::Rename { from, to } => todo!(),
                Command::Touch { file } => todo!(),
            }
        }

        Ok(())
    }
}
