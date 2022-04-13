use std::{os::unix::prelude::PermissionsExt, path::Path, str::FromStr};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use chrono::{DateTime, Local};
use directories::{BaseDirs, ProjectDirs};
use futures_util::StreamExt;
use getset::Getters;
use globset::Glob;
use log::{info, trace, warn};
use mime::Mime;
use mime_guess::MimeGuess;
use reqwest::{Client, Request};
use std::env::consts::ARCH;
use tokio::{fs::File, io::AsyncWriteExt, sync::Mutex};
use url::Url;
use walkdir::WalkDir;

use crate::{
    config::{Config, Hook},
    github::{self, Asset},
    updated_info::{Mapper, UpdatedInfo},
    util::{extract, find_one_exe_with_glob, run_cmd},
    Api,
};

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

trait Binary {
    fn name(&self) -> &str;

    fn version(&self) -> Version;

    fn exe_glob(&self) -> Option<&str>;

    fn hook(&self) -> Option<Hook>;
}

enum Version {
    Latest,
    Some(String),
}

#[derive(Debug, Getters)]
#[getset(get = "pub")]
struct BinaryManager<T: Api, B: Binary> {
    api: T,
    bin: B,
    mapper: Mapper,
    client: Client,
    config: Config,
    project_dirs: ProjectDirs,
    base_dirs: BaseDirs,
}

impl<T: Api, B: Binary> BinaryManager<T, B> {
    pub async fn install(&self) -> Result<()> {
        // download
        let url = self.api().installed_url().await?;

        let (cache_path, content_type) = self.download(url).await?;
        let to = self.project_dirs.data_dir().join(self.bin.name());
        {
            let (from, to) = (cache_path.clone(), to.clone());
            if let Some(x_cmd) = self
                .bin
                .hook()
                .as_ref()
                .and_then(|h| h.action().extract().as_deref())
            {
                run_cmd(x_cmd, cache_path).await?;
                // 解压到同名文件夹中 在移动到to中
                // let a = url
                //     .path_segments()
                //     .and_then(|seg| seg.last())
                //     .ok_or_else(|| anyhow!("not found filename for {}", url))?
                //     .parse::<PathBuf>()?.
                
            } else {
                tokio::task::spawn_blocking(move || extract(from, to, content_type.as_deref()))
                    .await??;
            }
        }

        self.link_to_exe_dir(to).await?;
        // hook
        todo!()
    }

    async fn link_to_exe_dir(&self, base: impl AsRef<Path>) -> Result<()> {
        let base = base.as_ref();
        let glob_pat = self
            .bin
            .exe_glob()
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("**/*{}*", self.bin.name()));

        let src = {
            let base = base.to_path_buf();
            tokio::task::spawn_blocking(move || find_one_exe_with_glob(&glob_pat, base)).await??
        };
        let dst = self.dst_link_path()?;

        info!("sym linking {} to {}", src.display(), dst.display());
        tokio::fs::symlink(src, dst).await?;
        Ok(())
    }

    async fn download(&self, url: &Url) -> Result<(PathBuf, Option<String>)> {
        let filename = url
            .path_segments()
            .and_then(|seg| seg.last())
            .map(ToString::to_string)
            .ok_or_else(|| anyhow!("not found filename for {}", url))?;

        let resp = self.client().get(url.as_ref()).send().await?;

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(ToString::to_string);
        let content_len = resp
            .content_length()
            .ok_or_else(|| anyhow!("not found content len for {}", url))?;

        let cache_path = self.project_dirs.cache_dir().join(filename);
        if let Ok(data) = tokio::fs::metadata(&cache_path).await {
            if data.len() == content_len {
                info!(
                    "found cached file {} with len {} for download url: {}",
                    cache_path.display(),
                    content_len,
                    url
                );
                return Ok((cache_path, content_type));
            } else {
                warn!(
                    "overwriting an existing file {} for url: {}",
                    cache_path.display(),
                    url
                );
            }
        }

        let mut file = File::create(&cache_path).await?;
        let mut stream = resp.bytes_stream();

        trace!("downloading to {} for url: {}", cache_path.display(), url);
        while let Some(chunk) = stream.next().await {
            file.write_all(&chunk?).await?;
        }

        Ok((cache_path, content_type))
    }

    fn dst_link_path(&self) -> Result<PathBuf> {
        let dst = self
            .config
            .executable_dir()
            .as_ref()
            .cloned()
            .or_else(|| self.base_dirs.executable_dir().map(|v| v.to_owned()))
            .ok_or_else(|| anyhow!("not found executable dir"))?
            .join(self.bin.name());
        Ok(dst)
    }
}

// #[async_trait]
// trait BinaryManager {
//     type API: Api;

//     fn api(&self) -> &Self::API;

//     fn mapper(&self) -> &Mapper;

//     fn binary_name(&self) -> &str;

//     fn client(&self) -> &Client;

//     fn config(&self) -> &Config;

//     async fn updated_infos(&self) -> Result<&[UpdatedInfo]>;

//     async fn last_updated_ver(&self) -> Result<Option<String>>;

//     async fn current_ver(&self) -> Result<Option<String>> {
//         self.last_updated_ver().await
//     }

//     async fn update(&self) -> Result<()>;

//     async fn uninstall(&self) -> Result<()>;
// }
