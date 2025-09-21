use crate::{
    config::{NodeConfig, NodeIdentifier},
    nullfs::Synchronizer,
};
use std::{path::PathBuf, sync::Arc};
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

mod config;
mod nullfs;
mod server;

#[cfg(test)]
mod tests;

#[actix_web::main]
async fn main() -> eyre::Result<()> {
    let args = std::env::args().collect::<Vec<String>>();

    let pkg_name = env!("CARGO_PKG_NAME").replace("-", "_");
    let pkg_version = env!("CARGO_PKG_VERSION");
    if args.len() < 2 {
        eprintln!("{pkg_name} {pkg_version}");
        eprintln!("Usage: {} <config-path>", args[0]);
        std::process::exit(1);
    }

    if std::env::var("RUST_LOG").is_err() {
        let filter_str = format!("{pkg_name}=info");
        unsafe {
            std::env::set_var("RUST_LOG", &filter_str);
        }
    }
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config_path = PathBuf::from(&args[1]);
    let config = Arc::new(NodeConfig::load_from_file(&config_path).await?);
    let identifier = Arc::new(NodeIdentifier::load_from_file(&PathBuf::from(format!(
        ".id-{}",
        config.name.trim()
    )))?);

    let shutdown = CancellationToken::new();
    let shutdown_sync = shutdown.clone();
    let sconfig = config.clone();
    let sidentifier = identifier.clone();
    let shutdown_server = shutdown.clone();

    let _ = tokio::spawn(async move { server::run(sconfig, sidentifier, shutdown_server).await });
    let _ = tokio::spawn(async move { Synchronizer::run(config, identifier, shutdown_sync).await });

    signal::ctrl_c().await?;
    shutdown.cancel();
    tracing::warn!("Shutting down everything...");

    Ok(())
}
