use std::path::Path;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use futures_util::StreamExt;
use getset::Getters;
use log::{debug, info, trace, warn};
use reqwest::Client;
use tokio::{
    fs::{self as afs, File},
    io::AsyncWriteExt,
};
use url::Url;

use crate::source::Binary;
use crate::source::Version;
use crate::{
    updated_info::{Mapper, UpdatedInfo},
    util::{extract, find_one_exe_with_glob, run_cmd},
};

#[derive(Debug, Getters)]
#[getset(get = "pub")]
struct BinaryManager<'a, B: Binary> {
    bin: B,
    mapper: &'a Mapper,
    client: Client,
    data_dir: &'a Path,
    cache_dir: &'a Path,
    executable_dir: &'a Path,
}

impl<'a, B: Binary> BinaryManager<'a, B> {
    pub async fn updateable_ver(&self) -> Result<Option<(String, String)>> {
        if !self.is_installed().await? {
            return Ok(None);
        }
        if let Version::Some(_) = self.bin.version() {
            return Ok(None);
        }

        let mut infos = self.mapper.select_list_by_name(self.bin.name()).await?;
        infos.sort_by(|a, b| b.create_time().cmp(a.create_time()));
        if let Some(info) = infos.first() {
            let latest_ver = self.bin.latest_ver().await?;
            if latest_ver > *info.version() {
                return Ok(Some((latest_ver, info.version().to_string())));
            }
        }
        Ok(None)
    }

    pub async fn update(&self) -> Result<()> {
        if let Some((new, old)) = self.updateable_ver().await? {
            info!("updating version {} from {}", new, old);
            self.uninstall().await?;
            self.install().await?;
            Ok(())
        } else {
            bail!("can not update")
        }
    }

    pub async fn uninstall(&self) -> Result<()> {
        let link = self.link_path();
        if let Err(e) = afs::remove_file(&link).await {
            debug!("failed to remove a link file {}: {}", link.display(), e);
        }

        let bin_dir = self.bin_data_dir();
        if let Err(e) = afs::remove_dir_all(&bin_dir).await {
            debug!("failed to remove a dir {}: {}", bin_dir.display(), e);
        }

        // TODO: remove temp dir of extract hook
        Ok(())
    }

    pub async fn install(&self) -> Result<()> {
        // download
        let ver = match self.bin.version() {
            Version::Latest => self.bin.latest_ver().await?.to_owned(),
            Version::Some(ver) => ver,
        };
        let url = self.bin.get_url(&ver).await?;
        info!(
            "installing {} version {} for url: {}",
            self.bin.name(),
            ver,
            url
        );

        let (download_path, content_type) = self.download(&url).await?;
        let to = self.bin_data_dir();
        afs::create_dir_all(&to).await?;

        // try use custom to extract
        self.extract(&download_path, &to, content_type).await?;

        // link to exe dir
        let src = {
            let glob_pat = self
                .bin
                .exe_glob()
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("**/*{}*", self.bin.name()));
            let base = to.to_path_buf();
            tokio::task::spawn_blocking(move || find_one_exe_with_glob(base, &glob_pat)).await??
        };
        let dst = self.link_path();
        if afs::metadata(&dst).await.is_ok() {
            bail!("found a existing file {} in exe dir", dst.display());
        }

        info!("sym linking {} to {}", src.display(), dst.display());
        tokio::fs::symlink(src, dst).await?;

        // inserto into db
        let info = UpdatedInfo::with_installed(self.bin.name(), &ver);
        debug!("inserting info to db: {:?}", info);
        self.mapper.insert(&info).await?;
        Ok(())
    }

    fn link_path(&self) -> PathBuf {
        self.executable_dir.join(self.bin.name())
    }

    fn bin_data_dir(&self) -> PathBuf {
        self.data_dir.join(self.bin.name())
    }

    /// 尝试解压from到to中
    ///
    /// 如果配置了extract hook，则使用自定义的cmd解压，在from级目录上可解压在`bin.{name,filename}`目录。
    /// 否则使用通用解压
    ///
    /// 如果之前已存在对应的目录，则不会解压直接返回认为是缓存
    ///
    /// # Error
    ///
    /// * 如果extract hook前中已存在`bin.{name,filename}`目录
    /// * 或之后不存在`bin.{name,filename}`目录
    /// * 如果无法使用通用解压
    async fn extract<P>(&self, from: P, to: P, content_type: Option<String>) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let (from, to) = (from.as_ref().to_owned(), to.as_ref().to_owned());
        let word_dir = from
            .parent()
            .ok_or_else(|| anyhow!("not found parent dir for: {}", from.display()))?;
        // try use custom to extract
        if let Some(cmd) = self
            .bin
            .hook()
            .as_ref()
            .and_then(|h| h.action().extract().as_deref())
        {
            // before: check if exists
            let paths = from
                .file_stem()
                .map(|name| word_dir.join(name))
                .into_iter()
                .chain(std::iter::once_with(|| {
                    word_dir.with_file_name(self.bin.name())
                }))
                .collect::<Vec<_>>();
            // if any exists
            for p in &paths {
                if afs::metadata(&p).await.is_ok() {
                    info!(
                        "use a existing path {} for extracting hook: {}",
                        p.display(),
                        cmd
                    );

                    if afs::metadata(&to).await.is_err() {
                        debug!("moving {} to {}", p.display(), to.display());
                        tokio::fs::rename(p, &to).await?;
                    }
                    return Ok(());
                }
            }

            info!("extracting with hook: {}", cmd);
            run_cmd(cmd, &word_dir).await?;

            for p in &paths {
                if afs::metadata(&p).await.is_ok() {
                    debug!("moving {} to {}", p.display(), to.display());
                    tokio::fs::rename(p, &to).await?;
                    return Ok(());
                }
            }

            bail!(
                "no decompression paths {:?} found after executing hook",
                paths
            );
        }
        // general extracting
        let (from, to) = (word_dir.to_owned(), to.to_owned());
        tokio::task::spawn_blocking(move || extract(from, to, content_type.as_deref())).await??;

        Ok(())
    }

    /// 下载url对应文件到缓存path
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

        let cache_path = self.cache_dir.join(filename);
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

    async fn is_installed(&self) -> Result<bool> {
        todo!()
    }
}
