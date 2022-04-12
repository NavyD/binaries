use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use chrono::{DateTime, Local};
use mime::Mime;
use mime_guess::MimeGuess;
use reqwest::{Client, Request};
use std::env::consts::ARCH;
use tokio::{sync::Mutex, fs::File, io::AsyncWriteExt};
use url::Url;
use futures_util::StreamExt;

use crate::{
    github::{config::Binary, Asset},
    updated_info::Mapper,
    Api, Config,
};

struct BinContxt<'a> {
    bin: GithubBinConfig,
    assets: Option<Vec<Asset>>,
    config: &'a Config,
}

impl<'a> BinContxt<'a> {
    async fn auto_select(&self) -> Option<&Asset> {
        // if self.assets.as_ref().map_or(true, Vec::is_empty) {
        //     return None;
        // }
        let assets = self.assets.as_ref()?;
        for asset in assets {}
        todo!()
    }
}

struct GithubBinConfig {
    owner: String,
    repo: String,
    version: Option<String>,
    tag: Option<String>,
    pattern: Option<String>,
}

struct Dir {}

#[async_trait]
trait BinaryManager {
    type API: Api;

    fn api(&self) -> &Self::API;

    fn mapper(&self) -> &Mapper;

    fn binary_name(&self) -> &str;

    fn client(&self) -> &Client;

    fn project_dir<P: AsRef<Path>>(&self) -> P;

    async fn last_updated_ver(&self) -> Result<Option<String>> {
        let mut vers = self.updated_vers().await?;
        vers.sort_by(|a, b| b.cmp(a));
        Ok(vers.first().cloned())
    }

    async fn updated_vers(&self) -> Result<Vec<String>> {
        self.mapper()
            .select_list_by_name(self.binary_name())
            .await
            .map(|a| a.into_iter().map(|e| e.version().to_owned()).collect())
            .map_err(Into::into)
    }

    async fn current_ver(&self) -> Result<Option<String>> {
        self.last_updated_ver().await
    }

    async fn install(&self) -> Result<()> {
        // download
        let url = self.api().installed_url().await?;
        let filename = url
            .path_segments()
            .and_then(|seg| seg.last())
            .map(ToString::to_string)
            .ok_or_else(|| anyhow!("not found filename for {}", url))?;

        let resp = self.client().get(url.as_ref()).send().await?;

        let content_len = resp
            .content_length()
            .ok_or_else(|| anyhow!("not found content len for {}", url))?;
        let cache_file_path = dirs::cache_dir()
            .map(|p| p.join(filename))
            .ok_or_else(|| anyhow!("not found cache dir"))?;

        if !cache_file_path.exists() {
            let mut file = File::create(cache_file_path).await?;
            let mut stream = resp.bytes_stream();

            while let Some(chunk) = stream.next().await {
                file.write_all(&chunk?).await?;
            }
        } else if cache_file_path.metadata()?.len() != content_len {
            bail!("Found an existing file: {}", cache_file_path.display());
        }
        // extract

        // link to exec path

        // hook
        todo!()
    }

    async fn update(&self) -> Result<()>;

    async fn uninstall(&self) -> Result<()>;
}
