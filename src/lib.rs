#![allow(unused)]

pub mod github;
pub mod manager;
pub mod updated_info;
pub mod util;
pub mod config;

pub static CRATE_NAME: &str = env!("CARGO_CRATE_NAME");

// read meta

// select one for local machine

// download

// extract

// install

use anyhow::Result;
use async_trait::async_trait;
use github::Binary;
use once_cell::sync::Lazy;
use url::Url;

#[async_trait]
trait Api: Sync {
    async fn latest_ver(&self) -> Result<&str>;

    async fn installed_url(&self) -> Result<&Url>;

    async fn updateable_url(&self) -> Result<&Url>;
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
