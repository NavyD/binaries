use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use anyhow::Result;
use async_trait::async_trait;
use reqwest::{Client, Request};
use std::env::consts::ARCH;
use tokio::sync::Mutex;
use url::Url;

#[async_trait]
trait GithubApi {
    fn owner(&self) -> &str;

    fn repo(&self) -> &str;

    fn base_url(&self) -> &Url;

    fn client(&self) -> &Client;

    async fn fetch_all_tags(&self) -> Result<Vec<String>> {
        let url = self.base_url().join("tags")?;
        todo!()
    }

    async fn fetch_all_releases(&self) -> Result<Vec<ReleaseInfo>> {
        let url = self.base_url().join("releases")?;
        todo!()
    }

    async fn fetch_latest_release(&self) -> Result<ReleaseInfo> {
        let url = self.base_url().join("releases/latest")?;
        todo!()
    }
}

pub struct ReleaseInfo {
    url: String,
    tag_name: String,
    name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

pub struct Asset {
    url: Url,
    name: String,
    size: usize,
    content_type: String,
    download_count: usize,
}

use getset::{Getters, Setters};

use crate::config::Hook;

#[derive(Debug, Getters, Setters)]
#[getset(get = "pub", set, get_mut)]
pub struct Binary {
    name: String,
    path: PathBuf,
    ver: String,
    hook: Option<Hook>,

    /// a glob of executable file in zip. for help to comfirm exe bin
    exe_glob: Option<String>,
}
