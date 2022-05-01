use anyhow::Result;
use async_trait::async_trait;

use url::Url;

use crate::config::Binary;

pub mod github;

#[async_trait]
pub trait Visible: std::fmt::Debug + Send + Sync {
    async fn latest_ver(&self) -> Result<String>;

    async fn get_url(&self, ver: &str) -> Result<Url>;

    fn bin(&self) -> &Binary;
    // async fn get_latest_url(&self) -> Result<Url> {
    //     self.get_url(&self.latest_ver().await?).await
    // }
}
