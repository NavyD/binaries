use std::collections::{HashMap, HashSet};

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Local};
use reqwest::{Client, Request};
use std::env::consts::ARCH;
use tokio::sync::Mutex;
use url::Url;

use crate::{Config, github::{Asset, config::Binary}, Api};

struct BinContxt<'a> {
    bin: GithubBinConfig,
    assets: Option<Vec<Asset>>,
    config: &'a Config,
}

impl<'a> BinContxt<'a> {
    async fn auto_select(&self) -> Option<&Asset> {
        // if self.assets.as_ref().map_or(true, Vec::is_empty) {
        //     return None;
        // }
        let assets = self.assets.as_ref()?;
        for asset in assets {}
        todo!()
    }
}

struct GithubBinConfig {
    owner: String,
    repo: String,
    version: Option<String>,
    tag: Option<String>,
    pattern: Option<String>,
}

#[async_trait]
trait BinaryManager {
    type API: Api;

    fn api(&self) -> &Self::API;

    // async fn check_for_updates(&self) -> Result<()> {
    //     let cur_ver = self.current_ver().await?;
    //     let latest_ver = self.api().latest_ver().await?;

    //     todo!()
    // }

    async fn last_updated_ver(&self) -> Result<Option<String>>;

    async fn updated_vers(&self) -> Result<Vec<String>>;

    async fn current_ver(&self) -> Result<Option<String>>;

    async fn install(&self) -> Result<()>;

    async fn update(&self) -> Result<()>;

    async fn uninstall(&self) -> Result<()>;
}
