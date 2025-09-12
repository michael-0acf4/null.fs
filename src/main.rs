use crate::{
    config::{NodeConfig, NodeIdentifier},
    netfs::{NetFsPath, Syncrhonizer},
};
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;

mod config;
mod netfs;
mod server;
mod tests;

#[actix_web::main]
async fn main() -> eyre::Result<()> {
    use camino::Utf8PathBuf;
    let path = NetFsPath::from_to_str(r"D:\a\b\c")?;

    println!("{}", path.to_string());
    println!("{}", path.to_host_path().display());
    println!("{}", NetFsPath::from(&path.to_host_path())?);

    // if std::env::var("RUST_LOG").is_err() {
    //     let filter_str = format!("{}=info", env!("CARGO_PKG_NAME").replace("-", "_"));
    //     unsafe {
    //         std::env::set_var("RUST_LOG", &filter_str);
    //     }
    // }
    // tracing_subscriber::fmt()
    //     .with_env_filter(EnvFilter::from_default_env())
    //     .without_time()
    //     .init();

    // let config = NodeConfig::load_from_file(Path::new("node-example.yaml")).await?;
    // let identifier = NodeIdentifier::load_from_file(&PathBuf::from(".id"))?;

    // tokio::try_join!(
    //     server::run(&config, &identifier),
    //     Syncrhonizer::run(&config, &identifier)
    // )?;

    Ok(())
}
