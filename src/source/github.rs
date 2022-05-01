use std::env::consts::{ARCH, OS};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use derive_builder::Builder;
use getset::Getters;
use log::{debug, log_enabled, trace, warn};
use mime::Mime;
use regex::Regex;
use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use url::Url;

use crate::{
    config::{Binary, Source},
    extract::SUPPORTED_CONTENT_TYPES,
    util::{get_archs, get_target_env, Templater},
};

use super::Visible;

/// [Rate limiting](https://docs.github.com/en/rest/overview/resources-in-the-rest-api#rate-limiting)
///
/// [Creating a token](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token#creating-a-token)
#[derive(Debug, Clone, Getters, Builder)]
#[getset(get = "pub")]
#[builder(setter(into), build_fn(name = "pre_build"))]
pub struct GithubBinary {
    client: Client,

    #[builder(setter(custom))]
    base_url: Url,

    binary: Binary,

    #[builder(default)]
    templater: Templater,
}

impl GithubBinaryBuilder {
    pub fn build(&mut self) -> Result<GithubBinary> {
        let url = self
            .binary
            .as_ref()
            .map(|bin| match bin.source() {
                Source::Github { owner, repo } => {
                    format!("https://api.github.com/repos/{}/{}/", owner, repo)
                }
            })
            .ok_or_else(|| anyhow!("not a github binary"))
            .and_then(|s| s.parse::<Url>().map_err(Into::into))?;
        self.base_url.replace(url);

        self.pre_build().map_err(Into::into)
    }
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

    fn bin(&self) -> &Binary {
        &self.binary
    }
}

/// [Releases The releases API allows you to create, modify, and delete releases and release assets.](https://docs.github.com/en/rest/reference/releases)
impl GithubBinary {
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
        let pick_re_fn = |hook| {
            let data = json!({
                "os": OS,
                "arch": ARCH,
                "target_env": get_target_env(),
            });
            trace!("rendering hook {} with data: {}", hook, data);
            let re = self
                .templater
                .render(hook, &data)
                .map(|s| s.trim().to_owned())?;
            if re.is_empty() {
                bail!("empty template");
            }
            debug!(
                "filtering {} assets by pick regex: {}",
                rel.assets().len(),
                re
            );
            let re = Regex::new(&re)?;
            let assets = rel
                .assets()
                .iter()
                .filter(|a| re.is_match(a.name()))
                .collect::<Vec<_>>();

            if log_enabled!(log::Level::Debug) {
                let names = assets
                    .iter()
                    .map(|a| a.name().to_owned())
                    .collect::<Vec<_>>()
                    .join(",");
                debug!(
                    "found {} assets by pick regex `{}`: {}",
                    assets.len(),
                    re,
                    names
                );
            }
            Ok(assets)
        };

        // filter by regex or name
        let mut assets = self
            .binary()
            .pick_regex()
            .as_deref()
            .map(pick_re_fn)
            .unwrap_or_else(|| {
                let conditions = [
                    // version like:   "tag_name": "0.6.8", "name": "0.6.8 Release",
                    vec![
                        self.binary().name().to_owned(),
                        rel.tag_name.to_owned(),
                        rel.name.to_owned(),
                    ],
                    vec![OS.to_owned()],
                    get_archs(),
                    vec![get_target_env().to_owned()],
                ];
                pick_by_name(rel.assets().iter(), &conditions).map(|v| v.collect::<Vec<_>>())
            })?;
        if assets.is_empty() {
            bail!("empty assets by regex or name");
        }

        if self
            .binary()
            .hook()
            .as_ref()
            .and_then(|h| h.extract().as_ref())
            .is_none()
        {
            // filter by content type
            let old_len = assets.len();
            trace!(
                "filtering {} assets by extract content types: {:?}",
                old_len,
                SUPPORTED_CONTENT_TYPES
            );

            assets.retain(|a| SUPPORTED_CONTENT_TYPES.contains(a.content_type()));

            if log_enabled!(log::Level::Debug) {
                debug!(
                    "filtered {} assets by extract content types: {}",
                    old_len - assets.len(),
                    assets
                        .iter()
                        .map(|a| a.name().to_owned())
                        .collect::<Vec<_>>()
                        .join(",")
                );
            }

            if assets.is_empty() {
                bail!("empty assets by supported content type",)
            }
        };

        if assets.len() == 1 {
            trace!("picked asset: {:?}", assets[0]);
            return Ok(assets[0]);
        }

        trace!("sorting {} assets by download count", assets.len());
        assets.sort_by(|a, b| b.download_count().cmp(a.download_count()));

        if log_enabled!(log::Level::Warn) {
            warn!(
                "found {} assets, pick `{}` asset for top of downloads: {}",
                assets.len(),
                assets[0].name(),
                assets
                    .iter()
                    .enumerate()
                    .map(|(i, a)| (i + 1).to_string()
                        + ":"
                        + a.name()
                        + ","
                        + &a.download_count().to_string())
                    .collect::<Vec<_>>()
                    .join(". ")
            );
        }

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
        trace!("fetching release with tag name `{}` for url: {}", tag, url);
        self.client
            .get(url)
            .send()
            .await?
            .json::<ResponseResult>()
            .await?
            .to()
    }
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
    trace!("picking by name with conditions: {:?}", conditions);
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
    }

    for step in (0..=conditions.len()).rev() {
        for i in (0..conditions.len()).step_by(step) {
            if i >= step {
                continue;
            }
            let re = get_regex(&conditions[i..step]);

            let re = regex::Regex::new(&re)?;
            if log_enabled!(log::Level::Trace) {
                let names = iter
                    .clone()
                    .map(|a| a.name().to_owned())
                    .collect::<Vec<_>>();
                trace!(
                    "picking {} assets by regex `{}`: {:?}",
                    names.len(),
                    re,
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

#[cfg(test)]
mod tests {
    use std::{fs::read_to_string, time::Duration};

    use log::info;
    use once_cell::sync::Lazy;
    use reqwest::{
        header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT},
        ClientBuilder,
    };

    use crate::config::BinaryBuilder;

    use super::*;

    static CLIENT: Lazy<Client> = Lazy::new(|| {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github.v3+json"),
        );
        let name = "Authorization";
        if let Ok(val) = std::env::var(name) {
            info!("loaded token {}={} for github rate limit", name, val);
            headers.insert(name, HeaderValue::from_str(&val).unwrap());
        }
        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/100.0.4896.88 Safari/537.36"));

        ClientBuilder::new()
            .timeout(Duration::from_secs(5))
            .default_headers(headers)
            .build()
            .expect("build client")
    });

    #[tokio::test]
    async fn test_fetch_latest_release() -> Result<()> {
        let bin = GithubBinaryBuilder::default()
            .client(CLIENT.clone())
            .binary(
                BinaryBuilder::default()
                    .source("github:Dreamacro/clash")?
                    .build()?,
            )
            .build()?;
        let release = bin.fetch_latest_release().await?;

        // latest at 22/4/17
        assert!(release.version() >= "v1.10.0");
        assert!(!release.prerelease);
        assert!(release.created_at >= "2022-03-19T05:58:51Z".parse::<DateTime<Utc>>()?);
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_release_by_tag_name() -> Result<()> {
        let bin = GithubBinaryBuilder::default()
            .client(CLIENT.clone())
            .binary(
                BinaryBuilder::default()
                    .source("github:Dreamacro/clash")?
                    .build()?,
            )
            .build()?;
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

        let _res: Release = serde_json::from_str::<ResponseResult>(&read_to_string(
            "tests/clash_latest_release.json",
        )?)?
        .to()?;

        let _res: Vec<Release> =
            serde_json::from_str::<ResponseResult>(&read_to_string("tests/clash_releases.json")?)?
                .to()?;
        Ok(())
    }

    #[test]
    fn test_pick_by_name() -> Result<()> {
        let bin = GithubBinaryBuilder::default()
            .client(CLIENT.clone())
            .binary(
                BinaryBuilder::default()
                    .source("github:Dreamacro/clash")?
                    .build()?,
            )
            .build()?;

        let conditions = &[
            vec![bin.bin().name().to_string()],
            vec!["linux".to_string()],
            get_archs(),
        ];
        let rel: Release = serde_json::from_str::<ResponseResult>(&read_to_string(
            "tests/clash_latest_release.json",
        )?)?
        .to()?;
        let res = pick_by_name(rel.assets().iter(), conditions)?;
        assert_eq!(res.clone().count(), 2);

        let conditions = &[
            vec![bin.bin().name().to_string()],
            vec!["linux".to_string()],
        ];
        let res = pick_by_name(rel.assets().iter(), conditions)?;
        assert_eq!(res.clone().count(), 13);

        // btm
        let bin = GithubBinaryBuilder::default()
            .client(CLIENT.clone())
            .binary(
                BinaryBuilder::default()
                    .source("github:ClementTsang/bottom")?
                    .build()?,
            )
            .build()?;
        let rel: Release = serde_json::from_str::<ResponseResult>(&read_to_string(
            "tests/bottom_latest_release.json",
        )?)?
        .to()?;
        let conditions = &[
            vec![
                bin.bin().name().to_owned(),
                rel.tag_name().to_owned(),
                rel.name().to_owned(),
            ],
            vec!["linux".to_string()],
            get_archs(),
        ];
        let res = pick_by_name(rel.assets().iter(), conditions)?;
        assert_eq!(res.clone().count(), 4);
        Ok(())
    }

    #[tokio::test]
    async fn test_pick_assets() -> Result<()> {
        let bin = GithubBinaryBuilder::default()
            .client(CLIENT.clone())
            .binary(
                BinaryBuilder::default()
                    .source("github:Dreamacro/clash")?
                    .build()?,
            )
            .build()?;

        let rel = bin.fetch_latest_release().await?;
        let asset = bin.pick_asset(&rel)?;

        #[cfg(target_os = "linux")]
        {
            assert!(asset.name().contains("linux"));
        }
        #[cfg(target_arch = "x86_64")]
        {
            assert!(asset.name().contains("amd64"));
        }

        let bin = GithubBinaryBuilder::default()
            .client(CLIENT.clone())
            .binary(
                BinaryBuilder::default()
                    .source("github:ClementTsang/bottom")?
                    // .hook(HookActionBuilder::default().pick("").build()?)
                    .build()?,
            )
            .build()?;

        let rel = bin.fetch_latest_release().await?;
        let asset = bin.pick_asset(&rel)?;
        #[cfg(target_os = "linux")]
        {
            assert!(asset.name().contains("linux"));
        }
        #[cfg(target_arch = "x86_64")]
        {
            assert!(asset.name().contains("x86_64"));
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_pick_by_hook() -> Result<()> {
        let bin = GithubBinaryBuilder::default()
            .client(CLIENT.clone())
            .binary(
                BinaryBuilder::default()
                    .source("github:ClementTsang/bottom")?
                    .pick_regex(
                        r#"
{{#if (and (eq os "linux") (eq arch "x86_64"))}}
bottom_x86_64-pc-windows-gnu.zip
{{else}}
bottom_x86_64-unknown-linux-gnu.tar.gz
{{/if}}"#,
                    )
                    .build()?,
            )
            .build()?;

        let rel = bin.fetch_latest_release().await?;
        let asset = bin.pick_asset(&rel)?;
        #[cfg(target_os = "linux")]
        {
            assert!(asset.name().contains("windows"));
        }
        Ok(())
    }
}
