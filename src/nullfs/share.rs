use std::{
    hash::{DefaultHasher, Hash, Hasher},
    str::FromStr,
    sync::Arc,
};

use crate::{
    config::{NodeIdentifier, RelayNode},
    nullfs::{
        Command, NullFs, NullFsPath, StashedCommand, any_fs::AnyFs, reduce_contiguous_subsequences,
    },
};
use chrono::{DateTime, Utc};
use eyre::Context;
use indexmap::IndexMap;
use sha2::{Digest, Sha256};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct ShareNode {
    pub store: Arc<CommandStash>,
    pub relay: RelayNode,
}

#[derive(Debug)]
pub struct CommandStash {
    pool: SqlitePool,
}

impl CommandStash {
    pub async fn new(identifier: &NodeIdentifier) -> eyre::Result<Self> {
        let options =
            SqliteConnectOptions::from_str(&format!("sqlite://.stash-{}.db", identifier.uuid))?
                // .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal) // adds a file
                // .pragma("synchronous", "NORMAL") // Less strict sync => adds a file
                .pragma("cache_size", "100000") // 100 000 pages (400 000kb)
                // .pragma("temp_store", "MEMORY") // Store temp tables in memory => induces sync issue with ReadLog
                .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS Command (
                id TEXT NOT NULL PRIMARY KEY,
                hash TEXT NOT NULL,
                command TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                volume TEXT NOT NULL,
                state INT NOT NULL
            );
        "#,
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    pub async fn stash(&self, commands: Vec<Command>, fs: &AnyFs) -> eyre::Result<()> {
        for command in commands {
            let to_stash = StashedCommand {
                id: Uuid::new_v4().to_string(),
                volume: fs.get_volume_name(),
                hash: {
                    let mut hasher = DefaultHasher::new();

                    let cmd = command.clone();
                    cmd.hash(&mut hasher);
                    let hash_id = hasher.finish();
                    let mut hasher = Sha256::new();
                    hasher.update(hash_id.to_string());
                    format!("{:x}", hasher.finalize())
                },
                command,
                timestamp: Utc::now(),
                state: 0,
            };

            sqlx::query(
                r#"
                INSERT INTO Command (id, hash, command, timestamp, volume, state)
                VALUES (?, ?, ?, ?, ?, ?)
            "#,
            )
            .bind(&to_stash.id)
            .bind(&to_stash.hash)
            .bind(serde_json::to_string(&to_stash.command).unwrap())
            .bind(to_stash.timestamp.to_rfc3339())
            .bind(to_stash.volume)
            .bind(to_stash.state)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn unstash(&self, volume: &str) -> eyre::Result<Vec<StashedCommand>> {
        let rows = sqlx::query(
            "SELECT id, hash, command, timestamp, volume, state
            FROM Command WHERE state = 0 AND volume = ?
            ORDER BY timestamp ASC",
        )
        .bind(volume)
        .fetch_all(&self.pool)
        .await?;

        let mut results = IndexMap::new();
        let mut contiguous_hashes = vec![];

        for row in rows {
            let id: String = row.try_get("id")?;
            let hash: String = row.try_get("hash")?;
            let cmd_str: String = row.try_get("command")?;
            let ts_str: String = row.try_get("timestamp")?;
            let volume: String = row.try_get("volume")?;
            let state: i32 = row.try_get("state")?;

            let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                .wrap_err_with(|| eyre::eyre!("Bad timestamp in row for hash {hash}"))?
                .with_timezone(&Utc);

            let command: Command = serde_json::from_str(&cmd_str)
                .wrap_err_with(|| eyre::eyre!("Parsing stored command for hash {hash}"))?;

            contiguous_hashes.push(hash.clone());
            results.insert(
                hash.clone(),
                StashedCommand {
                    id,
                    hash,
                    timestamp,
                    command,
                    volume,
                    state,
                },
            );
        }

        let collapsed_ops = reduce_contiguous_subsequences(&contiguous_hashes);
        Ok(collapsed_ops
            .iter()
            .map(|k| results.get(k).unwrap().clone())
            .collect())
    }

    pub async fn mark_done(&self, stashed: &StashedCommand) -> eyre::Result<()> {
        sqlx::query("UPDATE Command SET state = 5 WHERE id = ?1")
            .bind(&stashed.id)
            .execute(&self.pool)
            .await?;

        tracing::debug!("Operation done id={}, hash={}", stashed.id, stashed.hash);
        Ok(())
    }
}

impl ShareNode {
    pub async fn pull(&self, fs: &AnyFs, identifer: Arc<NodeIdentifier>) -> eyre::Result<()> {
        let client = reqwest::Client::new();
        let response = client
            .get(self.relay.address.join("v1/commands")?)
            .query(&[
                ("volume", fs.get_volume_name()),
                ("node_id", identifer.uuid.to_owned()),
            ])
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

        self.store.stash(external_changes, &fs).await?;

        Ok(())
    }

    pub async fn download(&self, path: &NullFsPath) -> eyre::Result<Vec<u8>> {
        let client = reqwest::Client::new();
        let response = client
            .get(self.relay.address.join("v1/download")?)
            .query(&[("path", path.to_string())])
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

    pub async fn ask_for_hash(&self, path: &NullFsPath) -> eyre::Result<String> {
        let client = reqwest::Client::new();
        let response = client
            .get(self.relay.address.join("v1/hash")?)
            .query(&[("path", path.to_string())])
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

    pub async fn run_command(&self, command: &Command, fs: &AnyFs) -> eyre::Result<()> {
        match command {
            Command::Delete { file } => fs.delete(file).await?,
            Command::Write { file } => {
                if file.stat.is_file() {
                    if fs.exists(&file.path).await? {
                        let remote_hash = self.ask_for_hash(&file.path).await?;
                        let local_hash = fs.hash(&file.path).await?;
                        if remote_hash == local_hash {
                            tracing::warn!(
                                "Already commited: Skipping touch update for {}",
                                file.path
                            );
                            return Ok(());
                        }
                    }

                    let data = self.download(&file.path).await?;
                    fs.write(file, &data).await?;
                } else {
                    fs.write(file, &[]).await?;
                }
            }
            Command::Touch { file } => {
                if fs.exists(&file.path).await? {
                    let remote_hash = self.ask_for_hash(&file.path).await?;
                    let local_hash = fs.hash(&file.path).await?;
                    if remote_hash == local_hash {
                        tracing::warn!("Already commited: Skipping touch update for {}", file.path);
                        return Ok(());
                    }

                    fs.delete(file).await?;
                }

                let data = self.download(&file.path).await?;
                fs.write(file, &data).await?;
            }
        };

        Ok(())
    }

    pub async fn apply_commands(&self, fs: &AnyFs) -> eyre::Result<()> {
        let stashed = self.store.unstash(&fs.get_volume_name()).await?;
        for op in stashed {
            let action = async {
                self.run_command(&op.command, fs).await?;
                self.store.mark_done(&op).await
            };

            if let Err(e) = action.await {
                tracing::error!("Failed {}: {}", op.command.to_string(), e);
            }
        }

        Ok(())
    }
}
