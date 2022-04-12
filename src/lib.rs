#![allow(unused)]

pub mod updated_info;
pub mod github;
pub mod manager;
pub mod util;

pub static CRATE_NAME: &str = env!("CARGO_CRATE_NAME");

// read meta

// select one for local machine

// download

// extract

// install

use anyhow::Result;
use async_trait::async_trait;
use github::config::Binary;
use once_cell::sync::Lazy;

#[async_trait]
trait Api: Sync {
    async fn latest_ver(&self) -> Result<String>;

    async fn download(&self, url: &str, path: &str) -> Result<()>;
}

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

pub struct Config {
    archs: Option<HashMap<String, HashSet<String>>>,
    github_bins: Vec<Binary>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::LevelFilter;
    use std::sync::Once;

    static INIT: Once = Once::new();

    #[ctor::ctor]
    fn init() {
        INIT.call_once(|| {
            env_logger::builder()
                .is_test(true)
                .filter_level(LevelFilter::Info)
                .filter_module(CRATE_NAME, LevelFilter::Trace)
                .init();
        });
    }
}
