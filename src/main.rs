use crate::{
    config::{NodeConfig, NodeIdentifier},
    netfs::{
        NetFs, NetFsPath, Syncrhonizer, any_fs::AnyFs, local_fs::LocalVolume, snapshot::Snapshot,
    },
};
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;

mod config;
mod netfs;
mod server;
mod tests;

#[actix_web::main]
async fn main() -> eyre::Result<()> {
    // let mut volume = LocalVolume {
    //     name: "Example".to_owned(),
    //     root: PathBuf::from(r"D:\a"),
    //     shares: vec![],
    // };
    // volume.init().await?;

    // let state_file = PathBuf::from("src/tests").join(format!("{}.state.json", volume.name));
    // let snap = Snapshot::new(AnyFs::Local { expose: volume });
    // let cmds = snap.capture(&state_file).await?;
    // for cmd in cmds {
    //     println!("{}", cmd.to_string());
    // }
    // if true {
    //     return Ok(());
    // }

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
    let identifier = NodeIdentifier::load_from_file(&PathBuf::from(".id"))?;

    tokio::try_join!(
        server::run(&config, &identifier),
        Syncrhonizer::run(&config, &identifier)
    )?;

    Ok(())
}
