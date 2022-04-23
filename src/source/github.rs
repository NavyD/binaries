use std::{env::consts::OS, fmt::Display, path::PathBuf, sync::Arc};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use derive_builder::Builder;
use futures_util::{FutureExt, StreamExt};
use getset::{Getters, Setters};
use log::{debug, error, log_enabled, trace, warn};
use mime::Mime;
use regex::Regex;
use reqwest::{header::HeaderValue, Client};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use url::Url;

use super::{Binary, Version, Visible};
use crate::{extract::SUPPORTED_CONTENT_TYPES, source::Hook, util::get_archs};

/// [Rate limiting](https://docs.github.com/en/rest/overview/resources-in-the-rest-api#rate-limiting)
///
/// [Creating a token](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token#creating-a-token)
#[derive(Debug, Clone, Getters)]
#[getset(get = "pub")]
pub struct GithubBinary {
    config: BinaryConfig,
    client: Client,
    base_url: Url,
}

#[async_trait]
impl Visible for GithubBinary {
    async fn latest_ver(&self) -> Result<String> {
        self.fetch_latest_release()
            .await
            .map(|rel| rel.version().to_owned())
    }

    async fn get_url(&self, ver: &str) -> Result<Url> {
        let release = self.fetch_release_by_tag_name(ver).await?;
        self.pick_asset(&release)?
            .browser_download_url
            .parse()
            .map_err(Into::into)
    }
}

impl Binary for GithubBinary {
    fn name(&self) -> &str {
        self.config
            .bin_name()
            .as_deref()
            .or_else(|| self.config.repo())
            .expect("not found bin name")
    }

    fn version(&self) -> super::Version {
        if let Some(v) = self.config.ver() {
            Version::Some(v.to_owned())
        } else {
            Version::Latest
        }
    }

    fn exe_glob(&self) -> Option<&str> {
        self.config.exe_glob().as_deref()
    }

    fn hook(&self) -> Option<&Hook> {
        self.config.hook().as_ref()
    }
}

/// [Releases The releases API allows you to create, modify, and delete releases and release assets.](https://docs.github.com/en/rest/reference/releases)
impl GithubBinary {
    pub fn new(client: Client, config: BinaryConfig) -> Self {
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
            base_url,
        }
    }

    /// 从release.assets中选择一个合适的asset。
    ///
    /// 如果配置了[pick_regex][BinaryConfig::pick_regex]则使用pick_regex过滤
    /// asset.name。否则使用通用的选择算法
    ///
    /// * bin-name, os, archs
    /// * content type
    /// * sort by download counts
    ///
    /// 注意：如果最后找到多个asset，将会使用下载数最高的asset
    ///
    /// # Error
    ///
    /// * 如果未找到任何asset
    fn pick_asset<'a>(&self, rel: &'a Release) -> Result<&'a Asset> {
        if let Some(re) = self.config.pick_regex().as_deref() {
            let re = Regex::new(re)?;
            let res = rel
                .assets()
                .iter()
                .filter(|a| re.is_match(a.name()))
                .collect::<Vec<_>>();
            if res.len() != 1 {
                error!(
                    "failed to pick asset by pick regex `{}`. found {} assets: {:?}",
                    re,
                    res.len(),
                    res
                );
                bail!("failed to pick asset by pick regex: `{}`", re)
            }
            return Ok(res[0]);
        }

        // filter by name
        let iter = pick_by_name(
            rel.assets().iter(),
            &[
                vec![rel.tag_name.to_owned(), rel.name.to_owned()],
                vec![OS.to_owned()],
                get_archs(),
            ],
        )?;
        if iter.clone().count() == 0 {
            bail!("picked empty by name");
        }

        let mut assets: Vec<_> = if let Some(cmd) = self
            .config
            .hook
            .as_ref()
            .and_then(|hook| hook.action.extract.as_deref())
        {
            debug!("skipped content type filter for extract hook: {}", cmd);
            iter.collect()
        } else {
            // filter by content type
            let iter = iter.filter(|a| SUPPORTED_CONTENT_TYPES.contains(a.content_type()));
            match iter.clone().count() {
                0 => bail!(
                    "picked empty by supported content type: {:?}",
                    SUPPORTED_CONTENT_TYPES
                ),
                _ => iter.collect(),
            }
        };

        if assets.len() == 1 {
            return Ok(rel
                .assets()
                .iter()
                .find(|a| *a == assets[0])
                .expect("not found in assets"));
        }
        assets.sort_by(|a, b| b.download_count().cmp(a.download_count()));
        warn!(
            "multiple assets {} are found, use the most downloads: 1.{},2.{}",
            assets.len(),
            assets[0].download_count(),
            assets[1].download_count()
        );
        Ok(assets[0])
    }

    async fn fetch_latest_release(&self) -> Result<Release> {
        let url = self.base_url.join("releases/latest")?;
        self.client
            .get(url)
            .send()
            .await?
            .json::<ResponseResult>()
            .await?
            .to()
    }

    /// [Get a release by tag name](https://docs.github.com/en/rest/reference/releases#get-a-release-by-tag-name)
    async fn fetch_release_by_tag_name(&self, tag: &str) -> Result<Release> {
        let url = self.base_url.join(&format!("releases/tags/{}", tag))?;
        self.client
            .get(url)
            .send()
            .await?
            .json::<ResponseResult>()
            .await?
            .to()
    }

    // 获取所有release可能会达到rate limit
    // async fn fetch_all_releases(&self) -> Result<Vec<Release>> {
    //     let url = self.base_url().join("releases")?;
    //     let mut releases = vec![];

    //     let (mut page, per_page) = (0, 30);
    //     loop {
    //         page += 1;
    //         let res = self
    //             .client
    //             .get(url.clone())
    //             .header("page", page.to_string().parse::<HeaderValue>()?)
    //             .send()
    //             .await?
    //             .json::<ResponseResult>()
    //             .await?
    //             .to::<Vec<Release>>()?;
    //         let len = res.len();
    //         releases.extend(res);
    //         if len < per_page {
    //             return Ok(releases);
    //         }
    //     }
    // }
}

/// Error: data did not match any variant of untagged enum ResponseResult
///
/// [Is there a way to allow an unknown enum tag when deserializing with Serde? [duplicate]](https://stackoverflow.com/a/63561656/8566831)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum ResponseResult {
    // important order
    Failed {
        #[serde(rename = "message")]
        message: String,
        #[serde(rename = "documentation_url")]
        documentation_url: String,
    },
    Ok(serde_json::Value),
}

impl ResponseResult {
    fn to<T>(self) -> Result<T>
    where
        T: DeserializeOwned,
    {
        match self {
            ResponseResult::Failed {
                message,
                documentation_url,
            } => Err(anyhow!(
                "message: {}, documentation_url: {}",
                message,
                documentation_url
            )),
            ResponseResult::Ok(val) => serde_json::from_value(val).map_err(Into::into),
        }
    }
}

// "{"message":"API rate limit exceeded for 1.65.204.86. (But here's the good news: Authenticated requests get a higher rate limit. Check out the documentation for more details.)","documentation_url":"https://docs.github.com/rest/overview/resources-in-the-rest-api#rate-limiting"}
// "
#[derive(Serialize, Deserialize, Debug, Clone, Getters, PartialEq, Eq)]
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

fn pick_by_name<'a, I>(
    iter: I,
    conditions: &[Vec<String>],
) -> Result<impl Iterator<Item = &'a Asset> + Clone>
where
    I: Iterator<Item = &'a Asset> + Clone,
{
    fn get_regex(conditions: &[Vec<String>]) -> String {
        let mut s = conditions
            .iter()
            .map(|w| w.join("|"))
            .collect::<Vec<_>>()
            .join("|");

        s.insert(0, '(');
        s += ").*";

        let mut re = String::new();
        for _ in 0..conditions.len() {
            re.push_str(&s);
        }
        re
    };

    for step in (0..=conditions.len()).rev() {
        for i in (0..conditions.len()).step_by(step) {
            let re = regex::Regex::new(&get_regex(&conditions[i..step]))?;
            if log_enabled!(log::Level::Trace) {
                let names = iter
                    .clone()
                    .map(|a| a.name().to_owned())
                    .collect::<Vec<_>>();
                trace!(
                    "picking assets by regex `{}` for {} names: {:?}",
                    re,
                    names.len(),
                    names.join(",")
                );
            }
            let iter = iter.clone().filter(move |a| re.is_match(a.name()));
            let res = iter.clone().collect::<Vec<_>>();
            if !res.is_empty() {
                if log_enabled!(log::Level::Trace) {
                    trace!(
                        "found {} assets: {}",
                        res.len(),
                        res.iter()
                            .map(|a| a.name().to_owned())
                            .collect::<Vec<_>>()
                            .join(",")
                    );
                }
                return Ok(iter);
            }
        }
    }
    bail!("not found asset by conditions {:?}", conditions)
}

#[derive(Serialize, Deserialize, Debug, Clone, Getters, PartialEq, Eq)]
#[getset(get = "pub")]
pub struct Asset {
    #[serde(rename = "id")]
    id: i64,

    /// file name
    #[serde(rename = "name")]
    name: String,

    #[serde(rename = "label")]
    label: Option<String>,

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

#[derive(Debug, Getters, Setters, Clone, Builder, Serialize, Deserialize)]
#[getset(get = "pub", set, get)]
#[builder(pattern = "mutable", setter(into, strip_option))]
pub struct BinaryConfig {
    name: String,

    #[builder(default)]
    bin_name: Option<String>,

    #[builder(default)]
    path: Option<PathBuf>,

    #[builder(default)]
    ver: Option<String>,

    #[builder(default)]
    hook: Option<Hook>,

    #[builder(default)]
    pick_regex: Option<String>,

    /// a glob of executable file in zip. for help to comfirm exe bin
    #[builder(default)]
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
    use std::{fs::read_to_string, iter, time::Duration};

    use log::info;
    use once_cell::sync::Lazy;
    use reqwest::{
        header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT},
        ClientBuilder,
    };

    use super::*;

    static BIN: Lazy<GithubBinary> = Lazy::new(|| {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github.v3+json"),
        );
        // max 100
        // headers.insert("per_page", HeaderValue::from_static("50"));
        // headers.insert("page", HeaderValue::from_static("1"));
        let name = "Authorization";
        if let Ok(val) = std::env::var(name) {
            info!("loaded token {}={} for github rate limit", name, val);
            headers.insert(name, HeaderValue::from_str(&val).unwrap());
        }
        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/100.0.4896.88 Safari/537.36"));

        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(5))
            .default_headers(headers)
            .build()
            .expect("build client");
        let config = BinaryConfigBuilder::default()
            .name("Dreamacro/clash")
            .build()
            .expect("building bin config");

        GithubBinary::new(client, config)
    });

    #[tokio::test]
    async fn test_fetch_latest_release() -> Result<()> {
        let bin = BIN.clone();
        let release = bin.fetch_latest_release().await?;

        // latest at 22/4/17
        assert!(release.version() >= "v1.10.0");
        assert!(!release.prerelease);
        assert!(release.created_at >= "2022-03-19T05:58:51Z".parse::<DateTime<Utc>>()?);
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_release_by_tag_name() -> Result<()> {
        let bin = BIN.clone();
        let tag = "v1.10.0";
        let rel = bin.fetch_release_by_tag_name(tag).await?;
        assert_eq!(rel.version(), tag);
        assert_eq!(rel.tag_name, tag);

        let res = bin.fetch_release_by_tag_name("_not_found__tag__").await;
        assert!(res.is_err());
        assert_eq!(
            res.map_err(|e| e.to_string().contains("Not Found")),
            Err(true)
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_choosen_one() -> Result<()> {
        Ok(())
    }

    #[test]
    fn test_serde_reponse_result() -> Result<()> {
        let rate_limit = r#"{
  "message": "Not Found",
  "documentation_url": "https://docs.github.com/rest"
}"#;
        let res: ResponseResult = serde_json::from_str(rate_limit)?;
        assert_eq!(
            res,
            ResponseResult::Failed {
                message: "Not Found".to_owned(),
                documentation_url: "https://docs.github.com/rest".to_owned()
            }
        );

        let res: Release = serde_json::from_str::<ResponseResult>(&read_to_string(
            "tests/clash_latest_release.json",
        )?)?
        .to()?;

        let res: Vec<Release> =
            serde_json::from_str::<ResponseResult>(&read_to_string("tests/clash_releases.json")?)?
                .to()?;
        Ok(())
    }

    #[test]
    fn test_pick() -> Result<()> {
        let conditions = &[
            vec![BIN.name().to_string()],
            vec!["linux".to_string()],
            get_archs(),
        ];
        let rel: Release = serde_json::from_str::<ResponseResult>(&read_to_string(
            "tests/clash_latest_release.json",
        )?)?
        .to()?;
        let res = pick_by_name(rel.assets().iter(), conditions)?;
        assert_eq!(res.clone().count(), 2);

        let conditions = &[vec![BIN.name().to_string()], vec!["linux".to_string()]];
        let res = pick_by_name(rel.assets().iter(), conditions)?;
        assert_eq!(res.clone().count(), 13);
        Ok(())
    }
}
