use std::path::PathBuf;

use anyhow::{bail, Result};
use async_trait::async_trait;
use derive_builder::Builder;
use getset::{Getters, MutGetters, Setters};
use serde::{Deserialize, Serialize};
use url::Url;

pub mod github;

#[async_trait]
pub trait Visible: std::fmt::Debug + Send + Sync {
    async fn latest_ver(&self) -> Result<String>;

    async fn get_url(&self, ver: &str) -> Result<Url>;

    // async fn get_latest_url(&self) -> Result<Url> {
    //     self.get_url(&self.latest_ver().await?).await
    // }
}
