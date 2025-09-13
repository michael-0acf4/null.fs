use crate::{
    config::{NodeConfig, NodeIdentifier},
    nullfs::Syncrhonizer,
};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

mod config;
mod nullfs;
mod server;

#[cfg(test)]
mod tests;

#[actix_web::main]
async fn main() -> eyre::Result<()> {
    let args = std::env::args().collect::<Vec<String>>();

    if args.len() < 2 {
        eprintln!("Usage: {} <config-path>", args[0]);
        std::process::exit(1);
    }

    if std::env::var("RUST_LOG").is_err() {
        let filter_str = format!("{}=info", env!("CARGO_PKG_NAME").replace("-", "_"));
        unsafe {
            std::env::set_var("RUST_LOG", &filter_str);
        }
    }
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config_path = PathBuf::from(&args[1]);
    let config = NodeConfig::load_from_file(&config_path).await?;
    let identifier =
        NodeIdentifier::load_from_file(&PathBuf::from(format!(".id-{}", config.name.trim())))?;

    tokio::try_join!(
        server::run(&config, &identifier),
        Syncrhonizer::run(&config, &identifier)
    )?;

    Ok(())
}
