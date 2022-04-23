use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Error;
use anyhow::{anyhow, bail, Result};
use derive_builder::Builder;
use futures_util::StreamExt;
use getset::Getters;
use handlebars::Handlebars;
use log::log_enabled;
use log::{debug, error, info, trace, warn};
use md5::{Digest, Md5};
use reqwest::Client;
use serde_json::json;
use tokio::fs::read_to_string;
use tokio::fs::remove_file;
use tokio::{
    fs::{self as afs},
    io::AsyncWriteExt,
};
use url::Url;
use which::which;

use crate::source::{Binary, Version};
use crate::{
    extract::decompress,
    updated_info::{Mapper, UpdatedInfo},
    util::find_one_exe_with_glob,
};

// struct BinaryContext {
//     bins: Vec<BinaryManager>,

// }

// impl BinaryContext {
//     pub fn install(&self) -> Result<()> {
//         for bin in &self.bins {
//             if !bin.has_installed().await? {
//                 tokio::spawn(|| async move {
//                     bin.latest_ver().await?;
//                     bin.install()
//                 });
//             }
//         }
//         todo!()
//     }
// }
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

#[derive(Debug, Getters, Builder, Clone)]
#[getset(get = "pub")]
pub struct BinaryPackage<'a, B: Binary> {
    bin: B,
    mapper: &'a Mapper,
    client: Client,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    executable_dir: PathBuf,
    template: &'a Handlebars<'a>,
}

impl<'a, B: Binary> BinaryPackage<'a, B> {
    pub async fn has_installed(&self) -> bool {
        let name = self.bin().name().to_owned();
        tokio::task::spawn_blocking(move || {
            which(&name).map_or(false, |p| {
                trace!("found executable bin {} in {}", name, p.display());
                true
            })
        })
        .await
        .expect("failed spawn blocking `which` task")
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
        let link = self.bin_link_path();
        trace!("removing link file {}", link.display());
        if let Err(e) = afs::remove_file(&link).await {
            info!("failed to remove a link file {}: {}", link.display(), e);
        }

        let bin_dir = self.bin_data_dir();
        trace!("removing data dir {}", bin_dir.display());
        if let Err(e) = afs::remove_dir_all(&bin_dir).await {
            info!("failed to remove data dir {}: {}", bin_dir.display(), e);
        }

        let cache_dir = self.bin_cache_dir();
        trace!("removing cache dir {}", cache_dir.display());
        if let Err(e) = afs::remove_dir_all(&cache_dir).await {
            info!("failed to remove cache dir {}: {}", cache_dir.display(), e);
        }

        // TODO: remove db
        Ok(())
    }

    pub async fn install(&self, ver: &str) -> Result<()> {
        let url = self.bin.get_url(ver).await?;
        info!("installing {} version {} for {}", self.bin.name(), ver, url);

        // download
        let download_path = self.download(&url).await?;
        let to = self.bin_data_dir();
        afs::create_dir_all(&to).await?;

        // try use custom to extract
        self.extract(&download_path, &to).await?;

        // link to exe dir
        self.link(&to).await?;

        // inserto into db
        let info = UpdatedInfo::with_installed(self.bin.name(), ver);
        debug!("inserting info to db: {:?}", info);
        self.mapper.insert(&info).await?;
        Ok(())
    }

    async fn link<P>(&self, to: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let src = {
            let base = to.as_ref().to_path_buf();
            let glob_pat = self
                .bin
                .exe_glob()
                .map(ToString::to_string)
                .unwrap_or_else(|| {
                    let pat = format!("**/*{}*", self.bin.name());
                    info!(
                        "use default glob pattern {} in directory {}",
                        pat,
                        base.display()
                    );
                    pat
                });
            tokio::task::spawn_blocking(move || find_one_exe_with_glob(base, &glob_pat)).await??
        };

        afs::create_dir_all(&self.executable_dir).await?;
        let dst = self.bin_link_path();

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

    fn bin_link_path(&self) -> PathBuf {
        self.executable_dir.join(self.bin.name())
    }

    fn bin_data_dir(&self) -> PathBuf {
        self.data_dir.join(self.bin.name())
    }

    fn bin_cache_dir(&self) -> PathBuf {
        self.cache_dir.join(self.bin.name())
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
        let cmd = self
            .bin
            .hook()
            .and_then(|h| h.action().extract().as_deref())
            .and_then(|cmd| {
                self.template
                    .render_template(
                        cmd,
                        &json!({
                            "from": from.as_ref().display().to_string(),
                            "to": to.as_ref().display().to_string()
                        }),
                    )
                    .map_err(|e| {
                        warn!("failed to render template `{}`: {}", cmd, e);
                    })
                    .ok()
            });

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

        let cache_dir = self.bin_cache_dir();
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
        let resp = self.client().get(url.as_ref()).send().await?;

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

    use crate::source::github::{BinaryConfig, GithubBinary};
    use crate::source::{github::BinaryConfigBuilder, HookActionBuilder, HookBuilder, Visible};

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
                    let sql = read_to_string("table_sqlite.sql").await?;
                    println!("setup sql: {}", sql);
                    let mut rows = sqlx::query(&sql).execute_many(&pool).await;
                    while let Some(row) = rows.try_next().await? {
                        println!("get row: {:?}", row);
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

    static PKG: Lazy<BinaryPackage<GithubBinary>> = Lazy::new(|| {
        let bin = BinaryConfigBuilder::default()
            .name("Dreamacro/clash")
            .build()
            .expect("building bin config");
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
            .timeout(Duration::from_secs(5))
            .default_headers(headers)
            .build()
            .expect("build client")
    });

    static HANDLEBARS: Lazy<Handlebars> = Lazy::new(Handlebars::new);

    fn create_pkg(config: BinaryConfig) -> Result<BinaryPackage<'static, GithubBinary>> {
        let bin = GithubBinary::new(BIN_CLIENT.clone(), config);

        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(10))
            .build()?;

        let new_path = env::join_paths(
            env::split_paths(&env::var("PATH").unwrap()).chain(once(EXE_DIR.clone())),
        )?;
        env::set_var("PATH", &new_path);

        std::fs::create_dir_all(&*CACHE_DIR)?;
        std::fs::create_dir_all(&*DATA_DIR)?;
        std::fs::create_dir_all(&*EXE_DIR)?;

        Ok(BinaryPackage {
            bin,
            client,
            cache_dir: CACHE_DIR.clone(),
            data_dir: DATA_DIR.clone(),
            executable_dir: EXE_DIR.clone(),
            mapper: &MAPPER,
            template: &HANDLEBARS,
        })
    }

    #[tokio::test]
    async fn test_install() -> Result<()> {
        let test_fn = |config| async move {
            let ver = "v12.1.2";
            // clear env path
            env::set_var("PATH", &env::join_paths(once(EXE_DIR.clone()))?);
            let pkg = create_pkg(config)?;

            assert!(which(pkg.bin.name()).is_err());

            pkg.install(ver).await?;

            let res = which(pkg.bin.name());
            assert!(res.is_ok());

            let out = Command::new(res.unwrap()).args(&["-V"]).output().await?;
            let s = std::str::from_utf8(&out.stdout)?;
            debug!("output: {}", s);
            assert!(s.contains(&ver[1..]));

            Ok::<_, Error>(())
        };
        let config = BinaryConfigBuilder::default()
            .name("XAMPPRocky/tokei")
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

            let to = DATA_DIR.clone();
            pkg.extract(&from, &to).await?;

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

        let config = BinaryConfigBuilder::default()
            .name("XAMPPRocky/tokei")
            .build()?;
        test_fn(config).await?;

        let config = BinaryConfigBuilder::default()
            .name("XAMPPRocky/tokei")
            .hook(
                HookBuilder::default()
                    .action(
                        HookActionBuilder::default()
                            .extract("tar xvf {{from}} -C {{to}}")
                            .build()?,
                    )
                    .build()?,
            )
            .build()?;
        test_fn(config).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_extract_when_hook() -> Result<()> {
        let config = BinaryConfigBuilder::default()
            .name("Dreamacro/clash")
            .hook(
                HookBuilder::default()
                    .action(
                        HookActionBuilder::default()
                            .extract("sh -c 'gzip -dc --keep {{ from }} > {{ to }}/clash'")
                            .build()?,
                    )
                    .build()?,
            )
            .build()?;

        let ver = "v1.10.0";
        let pkg = create_pkg(config)?;
        let url = pkg.bin.get_url(ver).await?;
        let from = pkg.download(&url).await?;

        let to = DATA_DIR.clone();
        pkg.extract(&from, &to).await?;

        assert!(to.join("clash").is_file());
        Ok(())
    }

    #[tokio::test]
    async fn test_download() -> Result<()> {
        let ver = "v1.10.0";
        let url = PKG.bin.get_url(ver).await?;
        let path = PKG.download(&url).await?;

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
