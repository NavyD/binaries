use std::{env::consts::OS, path::PathBuf, sync::Arc};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use getset::{Getters, Setters};
use log::{debug, error, trace};
use mime::Mime;
use reqwest::{header::HeaderValue, Client};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use typed_builder::TypedBuilder;
use url::Url;

use super::{Binary, Visible};
use crate::{binary::Hook, util::get_archs};

#[derive(Debug, Clone, Getters)]
#[getset(get = "pub")]
struct GithubBinary {
    config: BinaryConfig,
    client: Client,
    base_url: Url,

    /// [Rate limiting](https://docs.github.com/en/rest/overview/resources-in-the-rest-api#rate-limiting)
    token: Option<String>,
}

#[async_trait]
impl Visible for GithubBinary {
    async fn latest_ver(&self) -> Result<String> {
        self.fetch_latest_release()
            .await
            .map(|rel| rel.version().to_owned())
    }

    async fn get_url(&self, ver: Option<&str>) -> Result<Url> {
        if let Some(ver) = ver.map(str::trim) {
            let releases = self.fetch_all_releases().await?;
            trace!("got all releases: {:?}", releases);

            let res = releases
                .iter()
                .filter(|rel| rel.version().starts_with(ver))
                .collect::<Vec<_>>();

            let release = if res.len() > 1 {
                error!(
                    "found multiple {} releases by ver {}: {:?}",
                    res.len(),
                    ver,
                    res
                );
                bail!("found multiple {} releases by ver: {}", res.len(), ver);
            } else if res.is_empty() {
                error!(
                    "not found any releases by ver {} in releases: {:?}",
                    ver, releases
                );
                bail!("not found any releases by ver: {}", ver);
            } else {
                debug!("found a release {:?} by ver: {}", res[0], ver);
                res[0]
            };

            self.choosen_one(release).await
        } else {
            let release = self.fetch_latest_release().await?;
            self.choosen_one(&release).await
        }
    }
}

impl Binary for GithubBinary {
    fn name(&self) -> &str {
        todo!()
    }

    fn version(&self) -> super::Version {
        todo!()
    }

    fn exe_glob(&self) -> Option<&str> {
        todo!()
    }

    fn hook(&self) -> Option<Hook> {
        todo!()
    }
}

impl GithubBinary {
    pub fn new(client: Client, config: BinaryConfig, token: Option<&str>) -> Self {
        let base_url = config
            .owner()
            .zip(config.repo())
            .map(|(owner, repo)| format!("https://api.github.com/repos/{}/{}/", owner, repo))
            .expect("not found owner or repo")
            .parse::<Url>()
            .expect("url parse");

        Self {
            client,
            config,
            token: token.map(ToOwned::to_owned),
            base_url,
        }
    }

    async fn choosen_one(&self, rel: &Release) -> Result<Url> {
        let archs = get_archs();
        debug!(
            "choosing a url with archs: {:?}, bin name: {}, os: {} in assets: {:?}",
            archs,
            self.name(),
            OS,
            rel.assets()
        );

        let res = rel
            .assets()
            .iter()
            .filter(|a| a.name().contains(OS))
            .filter(|a| archs.iter().any(|arch| a.name().contains(arch)))
            .filter(|a| a.name().contains(self.name()))
            .collect::<Vec<_>>();

        if res.is_empty() {
            bail!("not found a asset")
        } else if res.len() > 1 {
            bail!("found multiple assets: {:?}", res);
        } else {
            res[0]
                .browser_download_url
                .parse::<Url>()
                .map_err(Into::into)
        }
    }

    async fn fetch_latest_release(&self) -> Result<Release> {
        let url = self.base_url.join("releases/latest")?;
        match self
            .client
            .get(url)
            .send()
            .await?
            .json::<ResponseResult>()
            .await?
        {
            ResponseResult::One(rel) => Ok(rel),
            ResponseResult::More(res) => bail!("more: {:?}", res),
            ResponseResult::Failed {
                message,
                documentation_url,
            } => bail!(
                "documentation url: {}, message: {}",
                documentation_url,
                message
            ),
        }
    }

    async fn fetch_all_releases(&self) -> Result<Vec<Release>> {
        let url = self.base_url().join("releases")?;
        let (mut page, per_page) = (0, 30);
        let mut releases = vec![];

        loop {
            page += 1;
            let res = match self
                .client
                .get(url.clone())
                .header("page", page.to_string().parse::<HeaderValue>()?)
                .send()
                .await?
                .json::<ResponseResult>()
                .await?
            {
                ResponseResult::One(res) => bail!("one: {:?}", res),
                ResponseResult::More(res) => res,
                ResponseResult::Failed {
                    message,
                    documentation_url,
                } => bail!(
                    "documentation url: {}, message: {}",
                    documentation_url,
                    message
                ),
            };
            let len = res.len();
            releases.extend(res);
            if len < per_page {
                return Ok(releases);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum ResponseResult {
    One(Release),
    More(Vec<Release>),
    Failed {
        message: String,
        documentation_url: String,
    },
}

// "{"message":"API rate limit exceeded for 1.65.204.86. (But here's the good news: Authenticated requests get a higher rate limit. Check out the documentation for more details.)","documentation_url":"https://docs.github.com/rest/overview/resources-in-the-rest-api#rate-limiting"}
// "
#[derive(Serialize, Deserialize, Debug, Clone, Getters)]
#[getset(get = "pub")]
pub struct Release {
    /// "url": "https://api.github.com/repos/Dreamacro/clash/releases/62241273",
    #[serde(rename = "id")]
    id: i64,

    #[serde(rename = "tag_name")]
    tag_name: String,

    #[serde(rename = "target_commitish")]
    target_commitish: String,

    #[serde(rename = "name")]
    name: String,

    #[serde(rename = "draft")]
    draft: bool,

    #[serde(rename = "prerelease")]
    prerelease: bool,

    #[serde(rename = "created_at")]
    created_at: DateTime<Utc>,

    #[serde(rename = "published_at")]
    published_at: DateTime<Utc>,

    #[serde(rename = "assets")]
    assets: Vec<Asset>,

    /// change log
    #[serde(rename = "body")]
    body: String,
}

impl Release {
    pub fn version(&self) -> &str {
        let (name, tag_name) = (self.name.trim(), self.tag_name.trim());
        if name.starts_with(&tag_name) {
            tag_name
        } else {
            name
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Getters)]
#[getset(get = "pub")]
pub struct Asset {
    #[serde(rename = "id")]
    id: i64,

    /// file name
    #[serde(rename = "name")]
    name: String,

    #[serde(rename = "label")]
    label: String,

    #[serde(
        rename = "content_type",
        deserialize_with = "hyper_serde::deserialize",
        serialize_with = "hyper_serde::serialize"
    )]
    content_type: Mime,

    #[serde(rename = "size")]
    size: i64,

    #[serde(rename = "download_count")]
    download_count: i64,

    #[serde(rename = "created_at")]
    created_at: DateTime<Utc>,

    #[serde(rename = "updated_at")]
    updated_at: DateTime<Utc>,

    #[serde(rename = "browser_download_url")]
    browser_download_url: String,
}

#[derive(Debug, Getters, Setters, Clone, TypedBuilder)]
#[getset(get = "pub", set, get)]
pub struct BinaryConfig {
    name: String,
    bin_name: Option<String>,
    path: PathBuf,
    ver: String,
    hook: Option<Hook>,

    /// a glob of executable file in zip. for help to comfirm exe bin
    exe_glob: Option<String>,
}

impl BinaryConfig {
    pub fn owner(&self) -> Option<&str> {
        self.name.split('/').next()
    }

    pub fn repo(&self) -> Option<&str> {
        self.name.split('/').last()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use once_cell::sync::Lazy;
    use reqwest::{
        header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT},
        ClientBuilder,
    };

    use super::*;

    static API: Lazy<GithubBinary> = Lazy::new(|| {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github.v3+json"),
        );
        // max 100
        headers.insert("per_page", HeaderValue::from_static("50"));
        headers.insert("page", HeaderValue::from_static("1"));
        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/100.0.4896.88 Safari/537.36"));

        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(5))
            .default_headers(headers)
            .build()
            .expect("build client");

        // GithubBinary::new(client, "Dreamacro", "clash")
        todo!()
    });

    // #[tokio::test]
    // async fn test_fetch_latest_release() -> Result<()> {
    //     let api = API.clone();
    //     let release = api.fetch_latest_release().await?;

    //     let create_at = "2022-03-19T05:58:51Z".parse::<DateTime<Utc>>()?;
    //     assert!(release.url().as_deref().unwrap().contains(&format!(
    //         "{}/{}",
    //         api.owner(),
    //         api.repo()
    //     )));
    //     assert!(!release.prerelease.unwrap());
    //     assert!(create_at <= release.created_at.unwrap());
    //     Ok(())
    // }

    // #[tokio::test]
    // async fn test_fetch_all_releases() -> Result<()> {
    //     let api = API.clone();
    //     let releases = api.fetch_all_releases().await?;

    //     assert!(releases.len() > 2);
    //     Ok(())
    // }
}
