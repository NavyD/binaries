use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use getset::{Getters, MutGetters, Setters};
use serde::{Deserialize, Serialize};
use url::Url;

pub mod github;

#[async_trait]
pub trait Visible {
    async fn latest_ver(&self) -> Result<String>;

    async fn get_url(&self, ver: &str) -> Result<Url>;

    async fn get_latest_url(&self) -> Result<Url> {
        self.get_url(&self.latest_ver().await?).await
    }
}

pub trait Binary: Visible + Clone {
    fn name(&self) -> &str;

    fn version(&self) -> Version;

    fn exe_glob(&self) -> Option<&str>;

    fn hook(&self) -> Option<&Hook>;
}

pub enum Version {
    Latest,
    Some(String),
}

#[derive(Debug, Getters, Builder, Setters, Clone, Serialize, Deserialize)]
#[getset(get = "pub")]
#[builder(pattern = "mutable", setter(into, strip_option))]
pub struct Hook {
    #[builder(default)]
    work_dir: Option<PathBuf>,
    action: HookAction,
}

#[derive(Debug, Getters, Builder, MutGetters, Clone, Serialize, Deserialize)]
#[getset(get = "pub", get_mut = "pub")]
#[builder(pattern = "mutable", setter(into, strip_option))]
pub struct HookAction {
    #[builder(default)]
    install: Option<String>,
    #[builder(default)]
    update: Option<String>,
    #[builder(default)]
    extract: Option<String>,
}
