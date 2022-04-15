use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use getset::{Getters, Setters};
use url::Url;

pub mod github;

#[async_trait]
pub trait Api: Sync {
    async fn latest_ver(&self) -> Result<&str>;

    async fn installed_url(&self) -> Result<&Url>;

    async fn updateable_url(&self) -> Result<&Url>;
}

pub trait Binary {
    fn name(&self) -> &str;

    fn version(&self) -> Version;

    fn exe_glob(&self) -> Option<&str>;

    fn hook(&self) -> Option<Hook>;
}

pub enum Version {
    Latest,
    Some(String),
}

#[derive(Debug, Getters, Setters)]
#[getset(get = "pub")]
pub struct Hook {
    work_dir: Option<PathBuf>,
    action: HookAction,
}

#[derive(Debug, Getters, Setters)]
#[getset(get = "pub")]
pub struct HookAction {
    install: Option<String>,
    update: Option<String>,
    extract: Option<String>,
}
