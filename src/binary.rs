use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use getset::{Getters, Setters};
use url::Url;

pub mod github;

#[async_trait]
pub trait Visible {
    async fn latest_ver(&self) -> Result<String>;

    async fn get_url(&self, ver: Option<&str>) -> Result<Url>;

    async fn get_latest_url(&self) -> Result<Url> {
        self.get_url(None).await
    }
}

pub trait Binary: Visible {
    fn name(&self) -> &str;

    fn version(&self) -> Version;

    fn exe_glob(&self) -> Option<&str>;

    fn hook(&self) -> Option<Hook>;
}

pub enum Version {
    Latest,
    Some(String),
}

#[derive(Debug, Getters, Setters, Clone)]
#[getset(get = "pub")]
pub struct Hook {
    work_dir: Option<PathBuf>,
    action: HookAction,
}

#[derive(Debug, Getters, Setters, Clone)]
#[getset(get = "pub")]
pub struct HookAction {
    install: Option<String>,
    update: Option<String>,
    extract: Option<String>,
}
