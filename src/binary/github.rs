use std::{path::PathBuf, sync::Arc};

use anyhow::{bail, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mime::Mime;
use reqwest::{header::HeaderValue, Client};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::binary::Hook;

use url::Url;

// #[async_trait]
// pub trait GithubApi {
//     fn owner(&self) -> &str;

//     fn repo(&self) -> &str;

//     fn base_url(&self) -> &Url;

//     fn client(&self) -> &Client;

//     async fn fetch_all_tags(&self) -> Result<Vec<String>> {
//         let _url = self.base_url().join("tags")?;
//         todo!()
//     }

//     async fn fetch_all_releases(&self) -> Result<Vec<ReleaseInfo>> {
//         let _url = self.base_url().join("releases")?;
//         todo!()
//     }

//     async fn fetch_latest_release(&self) -> Result<ReleaseInfo> {
//         let _url = self.base_url().join("releases/latest")?;
//         todo!()
//     }
// }

#[derive(Debug, Clone, Getters)]
#[getset(get = "pub")]
struct GithubApi {
    owner: String,
    repo: String,
    client: Client,
    base_url: Url,

    /// [Rate limiting](https://docs.github.com/en/rest/overview/resources-in-the-rest-api#rate-limiting)
    token: Option<String>,
}

fn new_api(owner: &str, repo: &str) -> Result<GithubApi> {
    todo!()
}

impl GithubApi {
    pub fn new(client: Client, owner: &str, repo: &str) -> Self {
        let url = format!("https://api.github.com/repos/{}/{}/", owner, repo)
            .parse()
            .expect("parse url");
        Self {
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            client,
            base_url: url,
            token: None,
        }
    }

    async fn fetch_latest_release(&self) -> Result<Release> {
        let url = self.base_url.join("releases/latest")?;
        self.client
            .get(url)
            .send()
            .await?
            .json::<Release>()
            .await
            .map_err(Into::into)
    }

    async fn fetch_all_releases(&self) -> Result<Vec<Release>> {
        let url = self.base_url().join("releases")?;
        let (mut page, per_page) = (1, 30);
        let mut releases = vec![];

        loop {
            let res = self
                .client
                .get(url.clone())
                .header("page", page.to_string().parse::<HeaderValue>()?)
                .send()
                .await?
                .json::<Vec<Release>>()
                .await?;
            let len = res.len();
            releases.extend(res);
            if len < per_page {
                return Ok(releases);
            }
            page += 1;
        }
    }
}

#[async_trait]
impl Api for GithubApi {
    async fn latest_ver(&self) -> Result<&str> {
        todo!()
    }

    async fn installed_url(&self) -> Result<&Url> {
        todo!()
    }

    async fn updateable_url(&self) -> Result<&Url> {
        todo!()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum ResponseResult {
    Ok(Box<Release>),
    Err {
        message: String,
        documentation_url: String,
    },
}

// "{"message":"API rate limit exceeded for 1.65.204.86. (But here's the good news: Authenticated requests get a higher rate limit. Check out the documentation for more details.)","documentation_url":"https://docs.github.com/rest/overview/resources-in-the-rest-api#rate-limiting"}
// "
#[derive(Serialize, Deserialize, Debug, Clone, Getters)]
#[getset(get = "pub")]
pub struct Release {
    #[serde(rename = "url")]
    url: Option<String>,

    #[serde(rename = "assets_url")]
    assets_url: Option<String>,

    #[serde(rename = "upload_url")]
    upload_url: Option<String>,

    #[serde(rename = "html_url")]
    html_url: Option<String>,

    #[serde(rename = "id")]
    id: Option<i64>,

    #[serde(rename = "author")]
    author: Option<Author>,

    #[serde(rename = "node_id")]
    node_id: Option<String>,

    #[serde(rename = "tag_name")]
    tag_name: Option<String>,

    #[serde(rename = "target_commitish")]
    target_commitish: Option<String>,

    #[serde(rename = "name")]
    name: Option<String>,

    #[serde(rename = "draft")]
    draft: Option<bool>,

    #[serde(rename = "prerelease")]
    prerelease: Option<bool>,

    #[serde(rename = "created_at")]
    created_at: Option<DateTime<Utc>>,

    #[serde(rename = "published_at")]
    published_at: Option<DateTime<Utc>>,

    #[serde(rename = "assets")]
    assets: Option<Vec<Asset>>,

    #[serde(rename = "tarball_url")]
    tarball_url: Option<String>,

    #[serde(rename = "zipball_url")]
    zipball_url: Option<String>,

    #[serde(rename = "body")]
    body: Option<String>,

    #[serde(rename = "reactions")]
    reactions: Option<Reactions>,

    #[serde(rename = "mentions_count")]
    mentions_count: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Getters)]
#[getset(get = "pub")]
pub struct Asset {
    #[serde(rename = "url")]
    url: Option<String>,

    #[serde(rename = "id")]
    id: Option<i64>,

    #[serde(rename = "node_id")]
    node_id: Option<String>,

    #[serde(rename = "name")]
    name: Option<String>,

    #[serde(rename = "label")]
    label: Option<String>,

    #[serde(rename = "uploader")]
    uploader: Option<Author>,

    #[serde(
        rename = "content_type",
        deserialize_with = "hyper_serde::deserialize",
        serialize_with = "hyper_serde::serialize"
    )]
    content_type: Mime,

    #[serde(rename = "state")]
    state: Option<String>,

    #[serde(rename = "size")]
    size: Option<i64>,

    #[serde(rename = "download_count")]
    download_count: Option<i64>,

    #[serde(rename = "created_at")]
    created_at: Option<String>,

    #[serde(rename = "updated_at")]
    updated_at: Option<String>,

    #[serde(rename = "browser_download_url")]
    browser_download_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Getters)]
#[getset(get = "pub")]
pub struct Author {
    #[serde(rename = "login")]
    login: Option<String>,

    #[serde(rename = "id")]
    id: Option<i64>,

    #[serde(rename = "node_id")]
    node_id: Option<String>,

    #[serde(rename = "avatar_url")]
    avatar_url: Option<String>,

    #[serde(rename = "gravatar_id")]
    gravatar_id: Option<String>,

    #[serde(rename = "url")]
    url: Option<String>,

    #[serde(rename = "html_url")]
    html_url: Option<String>,

    #[serde(rename = "followers_url")]
    followers_url: Option<String>,

    #[serde(rename = "following_url")]
    following_url: Option<String>,

    #[serde(rename = "gists_url")]
    gists_url: Option<String>,

    #[serde(rename = "starred_url")]
    starred_url: Option<String>,

    #[serde(rename = "subscriptions_url")]
    subscriptions_url: Option<String>,

    #[serde(rename = "organizations_url")]
    organizations_url: Option<String>,

    #[serde(rename = "repos_url")]
    repos_url: Option<String>,

    #[serde(rename = "events_url")]
    events_url: Option<String>,

    #[serde(rename = "received_events_url")]
    received_events_url: Option<String>,

    #[serde(rename = "type")]
    author_type: Option<String>,

    #[serde(rename = "site_admin")]
    site_admin: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Getters)]
#[getset(get = "pub")]
pub struct Reactions {
    #[serde(rename = "url")]
    url: Option<String>,

    #[serde(rename = "total_count")]
    total_count: Option<i64>,

    #[serde(rename = "+1")]
    the_1: Option<i64>,

    #[serde(rename = "-1")]
    reactions_1: Option<i64>,

    #[serde(rename = "laugh")]
    laugh: Option<i64>,

    #[serde(rename = "hooray")]
    hooray: Option<i64>,

    #[serde(rename = "confused")]
    confused: Option<i64>,

    #[serde(rename = "heart")]
    heart: Option<i64>,

    #[serde(rename = "rocket")]
    rocket: Option<i64>,

    #[serde(rename = "eyes")]
    eyes: Option<i64>,
}

use getset::{Getters, Setters};

use super::Api;

#[derive(Debug, Getters, Setters)]
#[getset(get = "pub", set, get)]
pub struct GithubBinary {
    name: String,
    path: PathBuf,
    ver: String,
    hook: Option<Hook>,

    /// a glob of executable file in zip. for help to comfirm exe bin
    exe_glob: Option<String>,
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

    static API: Lazy<GithubApi> = Lazy::new(|| {
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

        GithubApi::new(client, "Dreamacro", "clash")
    });

    #[tokio::test]
    async fn test_fetch_latest_release() -> Result<()> {
        let api = API.clone();
        let release = api.fetch_latest_release().await?;

        let create_at = "2022-03-19T05:58:51Z".parse::<DateTime<Utc>>()?;
        assert!(release.url().as_deref().unwrap().contains(&format!(
            "{}/{}",
            api.owner(),
            api.repo()
        )));
        assert!(!release.prerelease.unwrap());
        assert!(create_at <= release.created_at.unwrap());
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_all_releases() -> Result<()> {
        let api = API.clone();
        let releases = api.fetch_all_releases().await?;

        assert!(releases.len() > 2);
        Ok(())
    }
}
