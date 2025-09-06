use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use actix::Actor;
use tracing_subscriber::EnvFilter;

use crate::{
    actors::Runner,
    config::NodeConfig,
    netfs::{NetFs, Syncrhonizer, local_fs::LocalVolume, snapshot::Snapshot},
};

mod actors;
mod config;
mod netfs;
mod server;
mod tests;

#[actix_web::main]
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

    // let config = NodeConfig::load_from_file(Path::new("node-example.yaml")).await?;
    // let runner = Runner {
    //     config: config.clone(),
    // }
    // .start();

    // tokio::try_join!(server::run(&config, runner), Syncrhonizer::run(&config))?;

    let mut volume = LocalVolume {
        name: "Consoles".to_owned(),
        root: PathBuf::from("D:\\dev-env\\rust\\netfs\\src\\tests\\test_dir"),
        shares: vec![],
    };
    volume.init().await?;

    let snapshot = Snapshot::new(Arc::new(volume));
    let commands = snapshot.capture(&PathBuf::from(".state.json")).await?;
    for command in commands {
        println!("{}", command.to_string());
    }

    Ok(())
}
