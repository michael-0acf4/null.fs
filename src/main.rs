use std::path::Path;

use tracing_subscriber::EnvFilter;

use crate::config::NodeConfig;

mod config;
mod netfs;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        let filter_str = format!("{}=info", env!("CARGO_PKG_NAME").replace("-", "_"));
        unsafe {
            std::env::set_var("RUST_LOG", &filter_str);
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .without_time()
        .init();

    let config = NodeConfig::load_from_file(Path::new("node-example.yaml")).await?;

    println!("{}", serde_yaml::to_string(&config)?);
    println!("{:?}", config.resolve_alias("Cinnabar").unwrap());

    Ok(())
}
