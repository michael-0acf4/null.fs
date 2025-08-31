use crate::{config::NodeConfig, netfs::local_fs::LocalVolume};
use actix::{Actor, Context};

#[derive(Clone)]
pub struct Runner {
    pub config: NodeConfig,
}

#[derive(Clone)]
pub struct Watcher {
    pub volume: LocalVolume,
}

impl Actor for Watcher {
    type Context = Context<Self>;
}

impl Actor for Runner {
    type Context = Context<Self>;
}
