use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Error;
use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use futures_util::TryFutureExt;
use futures_util::{FutureExt, StreamExt};
use getset::Getters;
use log::{debug, error, info, trace, warn};
use md5::{Digest, Md5};
use reqwest::Client;
use tokio::fs::read_to_string;
use tokio::sync::Mutex;
use tokio::{
    fs::{self as afs},
    io::AsyncWriteExt,
};
use url::Url;
use which::which;

use crate::source::Binary;
use crate::source::Version;
use crate::{
    updated_info::{Mapper, UpdatedInfo},
    util::{extract, find_one_exe_with_glob, run_cmd},
};

// #[async_trait]
// pub trait Package: Sync {
//     type Bin: Binary;

//     fn bin(&self) -> &Self::Bin;

//     async fn has_installed(&self) -> bool {
//         let name = self.bin().name().to_owned();
//         tokio::task::spawn_blocking(move || {
//             which(&name).map_or(false, |p| {
//                 trace!("found executable bin {} in {}", name, p.display());
//                 true
//             })
//         })
//         .await
//         .unwrap_or_else(|e| {
//             error!("failed spawn blocking `which` task: {}", e);
//             false
//         })
//     }

//     async fn updateable_ver(&self) -> Option<(String, String)>;

//     async fn install(&self, ver: &str) -> Result<()>;

//     async fn uninstall(&self) -> Result<()>;

//     async fn update(&self) -> Result<()> {
//         if let Some((new, old)) = self.updateable_ver().await {
//             info!("updating version to {} from {}", new, old);
//             self.uninstall().await?;
//             self.install(&new).await?;
//             Ok(())
//         } else {
//             bail!("can not update")
//         }
//     }
// }

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
    pub async fn has_installed(&self) -> bool {
        let name = self.bin().name().to_owned();
        tokio::task::spawn_blocking(move || {
            which(&name).map_or(false, |p| {
                trace!("found executable bin {} in {}", name, p.display());
                true
            })
        })
        .await
        .unwrap_or_else(|e| {
            error!("failed spawn blocking `which` task: {}", e);
            false
        })
    }

    pub async fn updateable_ver(&self) -> Option<(String, String)> {
        if let Version::Some(_) = self.bin.version() {
            return None;
        }

        if !self.has_installed().await {
            return None;
        }

        let bin = self.bin.clone();
        let mapper = self.mapper.clone();
        let f = || async move {
            let mut infos = mapper.select_list_by_name(bin.name()).await?;
            infos.sort_by(|a, b| b.create_time().cmp(a.create_time()));
            if let Some(info) = infos.first() {
                let latest_ver = bin.latest_ver().await?;
                if latest_ver > *info.version() {
                    return Ok::<_, Error>(Some((latest_ver, info.version().to_string())));
                }
            }
            Ok(None)
        };
        f().await.unwrap_or(None)
    }

    async fn uninstall(&self) -> Result<()> {
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

    pub async fn install(&self, ver: &str) -> Result<()> {
        let url = self.bin.get_url(ver).await?;
        info!("installing {} version {} for {}", self.bin.name(), ver, url);

        // download
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

        let cache_path = self.cache_dir.join(&filename);
        let md5_path = self.cache_dir.join(&format!("{}.md5", filename));
        if afs::metadata(&cache_path).await.is_ok() {
            if afs::metadata(&md5_path).await.is_ok() {
                let (md5_digest, cache_path, md5_path) = (
                    read_to_string(&md5_path).await?,
                    cache_path.clone(),
                    md5_path.clone(),
                );
                if tokio::task::spawn_blocking(move || {
                    let mut hasher = Md5::new();
                    std::io::copy(&mut File::open(&md5_path)?, &mut hasher);
                    let digest: String = hasher
                        .finalize()
                        .iter()
                        .fold(String::new(), |a, e| a + &e.to_string());
                    trace!(
                        "found new digest {} and old {} for {}",
                        digest,
                        md5_digest,
                        md5_path.display()
                    );
                    Ok::<_, Error>(md5_digest == digest)
                })
                .await??
                {
                    info!("use cached file {}", cache_path.display());
                    return Ok((cache_path, None));
                } else {
                    warn!("inconsistent md5 digest");
                }
            } else {
                warn!("not found md5 digest in {}", md5_path.display());
            }
        }

        debug!("downloading {} for {}", filename, url);
        let resp = self.client().get(url.as_ref()).send().await?;

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(ToString::to_string);
        let content_len = resp
            .content_length()
            .ok_or_else(|| anyhow!("not found content len for {}", url))?;
        debug!(
            "response has content type: {:?}, content length: {} for {}",
            content_type, content_len, url
        );

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

        let mut file = afs::File::create(&cache_path).await?;
        let mut stream = resp.bytes_stream();

        trace!("downloading to {} for url: {}", cache_path.display(), url);
        while let Some(chunk) = stream.next().await {
            file.write_all(&chunk?).await?;
        }

        Ok((cache_path, content_type))
    }

    // async fn has_installed(&self) -> Result<bool> {
    //     todo!()
    // }
}
