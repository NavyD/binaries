use std::sync::Arc;

use anyhow::Result;
use binaries::{
    manager::{BinaryPackage, Package},
    source::Binary,
    updated_info::Mapper,
};
use directories::{BaseDirs, ProjectDirs};
use handlebars::Handlebars;
use reqwest::Client;

#[tokio::main]
async fn main() {
    //启用日志输出，你也可以使用其他日志框架，这个不限定的
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .filter_module(env!("CARGO_CRATE_NAME"), log::LevelFilter::Trace)
        .init();
}

struct Config {}

struct BinContext<P: Package<P> + Binary> {
    pkgs: Vec<Arc<P>>,
    template: Handlebars<'static>,
    project_dirs: ProjectDirs,
    base_dirs: BaseDirs,
    pkg_client: Client,
    mapper: Mapper,
    config: Config,
}

impl<P: Package<P> + 'static + Binary> BinContext<P> {
    fn new(config: Config) -> Result<Self> {
        todo!()
    }

    async fn install(&self) -> Result<()> {
        for pkg in &self.pkgs {
            let p = pkg.clone();
            tokio::spawn(async move {
                p.install("ver").await.unwrap();
            });
        }
        todo!()
    }
}
