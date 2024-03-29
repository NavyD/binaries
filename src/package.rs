use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Error;
use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use derive_builder::Builder;
use futures_util::StreamExt;
use getset::Getters;
use log::log_enabled;
use log::{debug, error, info, trace, warn};
use md5::{Digest, Md5};
use reqwest::Client;
use serde_json::json;
use tokio::fs::read_to_string;
use tokio::fs::remove_file;
use tokio::{fs as afs, io::AsyncWriteExt};
use url::Url;
use which::which;

use crate::config::Binary;
use crate::config::Source;
use crate::source::github::GithubBinaryBuilder;
use crate::source::Visible;

use crate::util::platform_values;
use crate::util::run_cmd;
use crate::util::Templater;
use crate::{
    extract::decompress,
    updated_info::{Mapper, UpdatedInfoBuilder},
    util::find_one_bin_with_glob,
};

#[derive(Debug, Clone, Builder, Getters)]
#[builder(build_fn(name = "pre_build"))]
#[getset(get = "pub")]
pub struct BinaryPackage {
    #[builder(setter(custom))]
    bin: Arc<Box<dyn Visible + 'static>>,
    mapper: Mapper,
    client: Client,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    link_path: PathBuf,
    #[builder(default)]
    templater: Templater,
}

impl BinaryPackageBuilder {
    pub fn bin(&mut self, bin: Binary) -> &mut Self {
        #[derive(Debug)]
        struct VisibleHelper {
            bin: Binary,
        }

        #[async_trait]
        impl Visible for VisibleHelper {
            async fn latest_ver(&self) -> Result<String> {
                unimplemented!()
            }

            async fn get_url(&self, _ver: &str) -> Result<Url> {
                unimplemented!()
            }

            fn bin(&self) -> &Binary {
                &self.bin
            }
        }

        self.bin = Some(Arc::new(Box::new(VisibleHelper { bin })));
        self
    }

    pub async fn build(&mut self) -> Result<BinaryPackage> {
        let bin = self
            .bin
            .take()
            .ok_or_else(|| anyhow!("no field bin"))?
            .bin()
            .clone();

        self.link_path = self.link_path.take().map(|p| p.join(bin.name()));

        let visible: Box<dyn Visible> = match bin.source() {
            Source::Github { owner: _, repo: _ } => Box::new(
                GithubBinaryBuilder::default()
                    .client(
                        self.client
                            .as_ref()
                            .ok_or_else(|| anyhow!("no field client"))?
                            .clone(),
                    )
                    .binary(bin)
                    .build()?,
            ),
        };
        self.bin.replace(Arc::new(visible));

        let mut pkg = self.pre_build()?;

        pkg.data_dir = pkg.data_dir.join(&format!("{}/", pkg.bin.bin().name()));
        pkg.cache_dir = pkg.cache_dir.join(&format!("{}/", pkg.bin.bin().name()));

        if afs::metadata(&pkg.link_path).await.is_err() {
            afs::create_dir_all(
                &pkg.link_path
                    .parent()
                    .ok_or_else(|| anyhow!("no parent for {}", pkg.link_path.display()))?,
            )
            .await?;
        }
        afs::create_dir_all(&pkg.data_dir).await?;
        afs::create_dir_all(&pkg.cache_dir).await?;
        Ok(pkg)
    }
}

impl BinaryPackage {
    pub async fn has_installed(&self) -> bool {
        let name = self.bin.bin().name().to_owned();
        let whiched = {
            let name = name.clone();
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
        };

        whiched
            && self
                .mapper
                .select_list_by_name(&name)
                .await
                .map_or(false, |v| {
                    trace!("found infos by name {}: {:?}", name, v);
                    !v.is_empty()
                })
    }

    pub async fn is_updateable(&self) -> bool {
        if self.bin.bin().version().is_some() || !self.has_installed().await {
            return false;
        }

        let name = self.bin.bin().name();
        match self
            .mapper
            .select_list_by_name(name)
            .await
            .and_then(|mut infos| {
                infos.sort_by(|a, b| b.create_time().cmp(a.create_time()));
                trace!("found {} infos by name {}: {:?}", infos.len(), name, infos);
                let first = infos
                    .first()
                    .cloned()
                    .ok_or_else(|| anyhow!("not found first in infos: {:?}", infos));
                debug!("found latest info by name {}: {:?}", name, first);
                first
            }) {
            Ok(info) => self
                .bin
                .latest_ver()
                .await
                .map(|latest| {
                    let cur = info.version();
                    trace!(
                        "checking current version: {} vs latest version: {}",
                        cur,
                        latest
                    );
                    &latest > cur
                })
                .unwrap_or(false),
            Err(e) => {
                warn!("failed to get info by name {}: {}", name, e);
                false
            }
        }
    }

    pub async fn install(&self) -> Result<()> {
        let ver = match self.bin.bin().version() {
            Some(ver) => ver.clone(),
            None => self.bin.latest_ver().await?,
        };
        let url = self.bin.get_url(&ver).await?;
        info!(
            "installing {} version {} for {}",
            self.bin.bin().name(),
            ver,
            url
        );

        // download
        let download_path = self.download(&url).await?;
        let to = &self.data_dir;
        if !afs::metadata(to).await.map_or(false, |d| d.is_dir()) {
            bail!("{} is not a dir", to.display());
        }

        // try use custom to extract
        self.extract(&download_path, to).await?;

        // link to exe dir
        self.link(&to).await?;

        // inserto into db
        let info = UpdatedInfoBuilder::default()
            .name(self.bin.bin().name())
            .source(serde_json::to_string(self.bin.bin().source())?)
            .url(url)
            .version(ver)
            .build()?;
        debug!("inserting info to db: {:?}", info);
        self.mapper.insert(&info).await?;

        if let Some(hook) = self
            .bin
            .bin()
            .hook()
            .as_ref()
            .and_then(|h| h.install().as_deref())
        {
            let data = platform_values(json!({
                "data_dir": self.data_dir.display().to_string(),
                "name": self.bin.bin().name(),
            }))?;
            let cmd = self.templater.render(hook, &data)?;
            run_cmd(&cmd, &self.data_dir).await?;
        }

        Ok(())
    }

    pub async fn uninstall(&self) -> Result<()> {
        trace!("removing link file {}", self.link_path.display());
        if let Err(e) = afs::remove_file(&self.link_path).await {
            info!(
                "failed to remove a link file {}: {}",
                self.link_path.display(),
                e
            );
        }

        trace!("removing data dir {}", self.data_dir.display());
        if let Err(e) = afs::remove_dir_all(&self.data_dir).await {
            info!(
                "failed to remove data dir {}: {}",
                self.data_dir.display(),
                e
            );
        }

        let name = self.bin.bin().name();
        trace!("deleting installed infos of {} from db", name);
        match self.mapper.delete_by_name(name).await {
            Ok(rows) => {
                if rows != 0 {
                    trace!("deleted {} infos of {}", rows, name);
                } else {
                    warn!("no info of {} removed", name);
                }
            }
            Err(e) => {
                info!("failed to delete info of {}: {}", name, e);
            }
        }

        if let Some(hook) = self
            .bin
            .bin()
            .hook()
            .as_ref()
            .and_then(|h| h.uninstall().as_deref())
        {
            let data = platform_values(json!({
                "data_dir": self.data_dir.display().to_string(),
                "name": self.bin.bin().name(),
            }))?;
            let cmd = self.templater.render(hook, &data)?;
            run_cmd(&cmd, &self.data_dir).await?;
        }
        Ok(())
    }

    pub async fn clean_cache(&self) -> Result<()> {
        let cache_dir = &self.cache_dir;
        trace!("removing cache dir {}", cache_dir.display());
        if let Err(e) = afs::remove_dir_all(&cache_dir).await {
            info!("failed to remove cache dir {}: {}", cache_dir.display(), e);
        }
        Ok(())
    }

    async fn link<P>(&self, to: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let dst = &self.link_path;
        if afs::metadata(dst).await.is_ok() {
            bail!("found the existing file {} for linking", dst.display());
        }

        let src = {
            let base = to.as_ref().to_path_buf();
            let glob_pat = self
                .bin
                .bin()
                .bin_glob()
                .as_ref()
                .map(|glob| {
                    let data = platform_values(json!({
                        "name": self.bin.bin().name(),
                    }))?;
                    self.templater.render(glob, &data).map(|pat| {
                        let s = pat.trim().to_owned();
                        debug!("use bin glob pattern {} in directory {}", s, base.display());
                        s
                    })
                })
                .unwrap_or_else(|| {
                    let pat = format!("**/*{}*", self.bin.bin().name());
                    warn!(
                        "use default glob pattern {} in directory {}",
                        pat,
                        base.display()
                    );
                    Ok(pat)
                })?;
            tokio::task::spawn_blocking(move || find_one_bin_with_glob(base, &glob_pat)).await??
        };

        if let Ok(d) = afs::metadata(&dst).await {
            error!(
                "found a existing path {} for linking. is link: {}",
                dst.display(),
                d.is_symlink()
            );
            bail!("a existing path {} for linking", dst.display());
        }

        info!("sym linking {} to {}", src.display(), dst.display());
        tokio::fs::symlink(src, dst).await?;
        Ok(())
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
    async fn extract<P>(&self, from: P, to: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let cmd = if let Some(hook) = self
            .bin
            .bin()
            .hook()
            .as_ref()
            .and_then(|h| h.extract().as_deref())
        {
            let data = platform_values(json!({
                "from": from.as_ref().display().to_string(),
                "to": to.as_ref().display().to_string(),
                "name": self.bin.bin().name(),
            }))?;
            Some(self.templater.render(hook, &data)?)
        } else {
            None
        };

        decompress(from, to, cmd.as_deref()).await
    }

    /// 下载url对应文件到缓存path
    ///
    /// 如果之前有下载过相同的文件且md5相同则使用缓存文件，否则重新下载
    async fn download(&self, url: &Url) -> Result<PathBuf> {
        let filename = url
            .path_segments()
            .and_then(|seg| seg.last())
            .map(ToString::to_string)
            .ok_or_else(|| anyhow!("not found filename for {}", url))?;

        let cache_dir = &self.cache_dir;
        afs::create_dir_all(&cache_dir).await?;

        let cache_path = cache_dir.join(&filename);
        let md5_path = cache_dir.join(&format!("{}.md5", filename));

        // check digest
        if afs::metadata(&cache_path).await.is_ok() {
            if afs::metadata(&md5_path).await.is_ok() {
                let is_identical = {
                    let (md5_digest, cache_path) =
                        (read_to_string(&md5_path).await?, cache_path.clone());
                    tokio::task::spawn_blocking(move || {
                        let mut hasher = Md5::new();
                        std::io::copy(&mut File::open(&cache_path)?, &mut hasher)?;
                        let digest: String = hasher
                            .finalize()
                            .iter()
                            .fold(String::new(), |a, e| a + &e.to_string());
                        trace!(
                            "found new digest {} and old {} for {}",
                            digest,
                            md5_digest,
                            cache_path.display()
                        );
                        Ok::<_, Error>(md5_digest == digest)
                    })
                    .await??
                };

                if is_identical {
                    info!("use cached file {}", cache_path.display());
                    return Ok(cache_path);
                } else {
                    warn!(
                        "inconsistent md5 digest. removing old cache {} and md5 {}",
                        cache_path.display(),
                        md5_path.display(),
                    );
                    remove_file(&cache_path).await?;
                    remove_file(&md5_path).await?;
                }
            } else {
                info!(
                    "not found md5 digest in {}. removing old cache {}",
                    md5_path.display(),
                    cache_path.display()
                );
                remove_file(&cache_path).await?;
            }
        }

        debug!("downloading {} for {}", filename, url);
        let resp = self.client.get(url.as_ref()).send().await?;

        if log_enabled!(log::Level::Trace) {
            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(ToString::to_string);
            trace!(
                "response has content type: {:?}, content length: {:?} for {}",
                content_type,
                resp.content_length(),
                url
            );
        }

        // create a new or truncate old
        let mut file = afs::File::create(&cache_path).await?;
        let mut stream = resp.bytes_stream();

        trace!("downloading to {} for url: {}", cache_path.display(), url);
        let mut hasher = Md5::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            hasher.update(chunk);
        }
        let digest = hasher
            .finalize()
            .iter()
            .fold(String::new(), |a, e| a + &e.to_string());

        trace!(
            "writing digest `{}` to {} for {}",
            digest,
            md5_path.display(),
            cache_path.display()
        );
        afs::write(&md5_path, digest).await?;

        Ok(cache_path)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs::Permissions, iter::once, os::unix::prelude::PermissionsExt, thread, time::Duration,
    };

    use futures_util::TryStreamExt;
    use once_cell::sync::Lazy;
    use reqwest::{
        header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT},
        ClientBuilder,
    };
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::{tempdir, TempDir};
    use tokio::{
        fs::{create_dir_all, write},
        process::Command,
        runtime::Runtime,
    };

    use crate::config::{Binary, BinaryBuilder, HookActionBuilder};

    use super::*;

    pub static TOKIO_RT: Lazy<Runtime> = Lazy::new(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    });

    static MAPPER: Lazy<Mapper> = Lazy::new(|| {
        thread::spawn(|| {
            let pool = TOKIO_RT
                .block_on(async {
                    let pool = SqlitePoolOptions::new()
                        .max_connections(4)
                        .connect("sqlite::memory:")
                        .await?;
                    let sql =
                        read_to_string("schema.sql").await? + &read_to_string("data.sql").await?;
                    trace!("setup sql: {}", sql);
                    let mut rows = sqlx::query(&sql).execute_many(&pool).await;
                    while let Some(row) = rows.try_next().await? {
                        trace!("get row: {:?}", row);
                    }
                    Ok::<_, Error>(pool)
                })
                .unwrap();

            Mapper { pool }
        })
        .join()
        .unwrap()
    });

    static TEMP: Lazy<TempDir> = Lazy::new(|| tempdir().unwrap());

    static CACHE_DIR: Lazy<PathBuf> = Lazy::new(|| TEMP.path().join("cache_dir"));
    static DATA_DIR: Lazy<PathBuf> = Lazy::new(|| TEMP.path().join("data_dir"));
    static EXE_DIR: Lazy<PathBuf> = Lazy::new(|| TEMP.path().join("exe_dir"));

    static PKG: Lazy<BinaryPackage> = Lazy::new(|| {
        let bin = BinaryBuilder::default()
            .source("github:Dreamacro/clash")
            .unwrap()
            .build()
            .unwrap();
        create_pkg(bin).unwrap()
    });

    static BIN_CLIENT: Lazy<Client> = Lazy::new(|| {
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
            .timeout(Duration::from_secs(10))
            .default_headers(headers)
            .build()
            .expect("build client")
    });

    fn create_pkg(bin: Binary) -> Result<BinaryPackage> {
        let client = BIN_CLIENT.clone();

        let new_path = env::join_paths(
            env::split_paths(&env::var("PATH").unwrap()).chain(once(EXE_DIR.clone())),
        )?;
        env::set_var("PATH", &new_path);

        let f = || {
            let data_dir = DATA_DIR.to_owned();
            let exe_dir = EXE_DIR.to_owned();
            let cache_dir = CACHE_DIR.to_owned();
            let mapper = MAPPER.clone();
            async move {
                BinaryPackageBuilder::default()
                    .bin(bin)
                    .data_dir(data_dir)
                    .link_path(exe_dir)
                    .cache_dir(cache_dir)
                    .client(client)
                    .mapper(mapper)
                    .build()
                    .await
            }
        };
        let bin_pkg = thread::spawn(|| TOKIO_RT.block_on(f())).join().unwrap()?;
        Ok(bin_pkg)
    }

    #[tokio::test]
    async fn test_install() -> Result<()> {
        let test_fn = |config| async move {
            let ver = "v12.1.2";
            // clear env path
            env::set_var("PATH", &env::join_paths(once(EXE_DIR.clone()))?);
            let pkg = create_pkg(config)?;

            assert!(which(pkg.bin.bin().name()).is_err());

            pkg.install().await?;

            let res = which(pkg.bin.bin().name());
            assert!(res.is_ok());

            let out = Command::new(res.unwrap()).args(&["-V"]).output().await?;
            let s = std::str::from_utf8(&out.stdout)?;
            debug!("output: {}", s);
            assert!(s.contains(&ver[1..]));

            Ok::<_, Error>(())
        };
        let config = BinaryBuilder::default()
            .source("github:XAMPPRocky/tokei")?
            .build()?;

        test_fn(config).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_extract() -> Result<()> {
        let test_fn = |config| async move {
            let ver = "v12.1.2";
            let pkg = create_pkg(config)?;
            let url = pkg.bin.get_url(ver).await?;
            let from = pkg.download(&url).await?;

            let to = &pkg.data_dir;
            pkg.extract(&from, to).await?;

            // let mut dirs = afs::read_dir(&to).await?;
            let mut found = false;
            while let Some(dir) = afs::read_dir(&to).await?.next_entry().await? {
                if dir.metadata().await.map(|p| p.is_file()).unwrap_or(false)
                    && dir
                        .file_name()
                        .to_str()
                        .map(|s| s == "tokei")
                        .unwrap_or(false)
                {
                    found = true;
                    break;
                }
            }
            assert!(found);
            Ok::<_, Error>(())
        };

        let config = BinaryBuilder::default()
            .source("github:XAMPPRocky/tokei")?
            .build()?;
        test_fn(config).await?;

        let config = BinaryBuilder::default()
            .source("github:XAMPPRocky/tokei")?
            .hook(
                HookActionBuilder::default()
                    .extract("tar xvf {{from}} -C {{to}}")
                    .build()?,
            )
            .build()?;
        test_fn(config).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_extract_when_hook() -> Result<()> {
        let config = BinaryBuilder::default()
            .source("github:Dreamacro/clash")?
            .hook(
                HookActionBuilder::default()
                    .extract("sh -c 'gzip -dc --keep {{ from }} > {{ to }}/clash'")
                    .build()?,
            )
            .build()?;

        let ver = "v1.10.0";
        let pkg = create_pkg(config)?;
        let url = pkg.bin.get_url(ver).await?;
        let from = pkg.download(&url).await?;

        pkg.extract(&from, &pkg.data_dir).await?;

        assert!(pkg.data_dir.join("clash").is_file());
        Ok(())
    }

    #[tokio::test]
    async fn test_download() -> Result<()> {
        let bin = BinaryBuilder::default()
            .source("github:Dreamacro/clash")?
            .build()?;

        let ver = "v1.10.0";
        let pkg = create_pkg(bin).expect("test error");
        let url = pkg.bin.get_url(ver).await?;
        let path = pkg.download(&url).await?;

        assert!(path.is_file());
        assert_eq!(
            path.file_name().and_then(|p| p.to_str()),
            url.path_segments().and_then(|p| p.last())
        );

        let _ = PKG.download(&url).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_exe_path() -> Result<()> {
        let bin_name = "bin_exe";
        assert!(which(bin_name).is_err());

        let exe_path = TEMP.path().join("exe");
        create_dir_all(&exe_path).await?;

        let exe_file = exe_path.join(bin_name);
        let content = r#"
#!/bin/sh
echo 'hello'"#;
        write(&exe_file, content).await?;

        afs::set_permissions(&exe_file, Permissions::from_mode(0o770)).await?;

        let path = env::var("PATH")?;
        let mut paths = env::split_paths(&path).collect::<Vec<_>>();
        paths.push(exe_path);
        let new_path = env::join_paths(paths)?;
        env::set_var("PATH", &new_path);

        assert_eq!(which(bin_name), Ok(exe_file));
        Ok(())
    }
}
