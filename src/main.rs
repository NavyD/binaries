use std::{
    path::{Path, PathBuf},
    process::exit,
    time::Duration,
};

use anyhow::{anyhow, bail, Error, Result};
use binaries::{
    config::{Binary, BinaryBuilder, Config, Source},
    manager::{BinaryPackage, BinaryPackageBuilder},
    updated_info::Mapper,
    CRATE_NAME,
};
use clap::{Args, Parser, Subcommand};
use directories::{BaseDirs, ProjectDirs};
use futures_util::{
    future::{join_all, try_join_all},
    StreamExt,
};
use log::{debug, error, info, trace, warn};
use once_cell::sync::Lazy;
use reqwest::{
    header::{self, HeaderMap},
    Client, ClientBuilder,
};
use sqlx::{sqlite::SqlitePoolOptions, Executor};

use tokio::{
    fs::{self as afs, create_dir_all},
    task::JoinHandle,
};

static PROJECT_DIRS: Lazy<ProjectDirs> =
    Lazy::new(|| ProjectDirs::from("xyz", "navyd", CRATE_NAME).expect("no project dirs"));

#[tokio::main]
async fn main() {
    if let Err(e) = Opt::parse().run().await {
        eprintln!("failed to run: {}", e);
        exit(1);
    }
}

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct Opt {
    #[clap(short, long, parse(from_occurrences))]
    verbose: u8,

    #[clap(short = 'f', long)]
    config_path: Option<PathBuf>,

    #[clap(subcommand)]
    commands: Commands,
}

impl Opt {
    async fn run(&self) -> Result<()> {
        self.init_log()?;
        let config = self.load_config().await?;

        let pm = PackageManager::new(config).await?;
        match &self.commands {
            Commands::Install => pm.install().await?,
            Commands::Uninstall(args) => pm.uninstall(args).await?,
            _ => {}
        }
        Ok(())
    }

    async fn load_config(&self) -> Result<Config> {
        let path = self
            .config_path
            .as_deref()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| PROJECT_DIRS.config_dir().join("config.yaml"));

        info!("loading config from {}", path.display());
        let config = afs::read_to_string(path).await?;
        trace!("loaded config str: {}", config);
        serde_yaml::from_str(&config).map_err(Into::into)
    }

    fn init_log(&self) -> Result<()> {
        let verbose = self.verbose;
        if verbose > 4 {
            return Err(anyhow!("invalid arg: 4 < {} number of verbose", verbose));
        }
        let level: log::LevelFilter = unsafe { std::mem::transmute((verbose + 1) as usize) };
        env_logger::builder()
            .filter_level(log::LevelFilter::Error)
            .filter_module(module_path!(), level)
            .init();
        Ok(())
    }
}

#[derive(Debug, Subcommand)]
enum Commands {
    List,
    Check,
    Update,
    Install,
    Uninstall(UninstallArgs),
    Clean,
}

#[derive(Debug, Args)]
pub struct UninstallArgs {
    #[clap(short, long)]
    names: Option<Vec<String>>,

    #[clap(short, long)]
    all: bool,
}

#[derive(Debug, Clone)]
pub struct PackageManager {
    bin_pkgs: Vec<BinaryPackage>,
}

impl PackageManager {
    pub async fn new(config: Config) -> Result<Self> {
        let project_dirs = PROJECT_DIRS.clone();
        let base_dirs = BaseDirs::new().ok_or_else(|| anyhow!("no base dirs"))?;

        let client = build_client()?;
        let mapper =
            build_mapper(project_dirs.data_dir().join(&format!("{}.db", CRATE_NAME))).await?;

        let build_pkg = |bin| {
            let (data_dir, cache_dir, executable_dir) = (
                project_dirs.data_dir().to_owned(),
                project_dirs.cache_dir().to_owned(),
                base_dirs.executable_dir().map(ToOwned::to_owned),
            );
            let client = client.clone();
            let mapper = mapper.clone();
            async move {
                BinaryPackageBuilder::default()
                    .bin(bin)
                    .data_dir(data_dir.to_owned())
                    .link_path(executable_dir.ok_or_else(|| anyhow!("no exe dir"))?)
                    .cache_dir(cache_dir.to_owned())
                    .client(client)
                    .mapper(mapper)
                    .build()
                    .await
            }
        };

        // build packages
        let bin_pkgs = try_join_all(
            config
                .bins()
                .iter()
                .map(Clone::clone)
                .map(build_pkg)
                .map(tokio::spawn),
        )
        .await?
        .into_iter()
        .collect::<Result<Vec<BinaryPackage>>>()?;

        trace!("got {} bin packages", bin_pkgs.len());

        // uninstall unused bins
        join_all(
            unused_bins(&mapper, config.bins())
                .await?
                .into_iter()
                .map(build_pkg)
                .map(|f| async move {
                    let pkg: BinaryPackage = f.await?;
                    info!("uninstalling unused binary {}", pkg.bin().bin().name());
                    pkg.uninstall()
                        .await
                        .map(|_| pkg.bin().bin().name().to_owned())
                        .map_err(|e| {
                            anyhow!(
                                "failed to uninstall unused bin {}: {}",
                                pkg.bin().bin().name(),
                                e
                            )
                        })
                })
                .map(tokio::spawn),
        )
        .await
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .map(|r| r.as_deref())
        .for_each(|r: Result<&str, _>| match r {
            Ok(name) => debug!("uninstalled bin {} of unused", name),
            Err(e) => warn!("{}", e),
        });

        Ok(Self { bin_pkgs })
    }

    pub async fn uninstall(&self, args: &UninstallArgs) -> Result<()> {
        if args.all {
            try_join_all(
                self.bin_pkgs
                    .iter()
                    .map(Clone::clone)
                    .map(|pkg| async move {
                        let name = pkg.bin().bin().name();

                        pkg.uninstall().await.map(|_| name.to_owned())
                    })
                    .map(tokio::spawn),
            )
            .await?
            .into_iter()
            .for_each(|r: Result<_>| match r {
                Ok(name) => info!("uninstalled {}", name),
                Err(e) => error!("{}", e),
            });
            return Ok(());
        }

        if let Some(names) = &args.names {
            let jobs = names
                .iter()
                .flat_map(|name| {
                    self.bin_pkgs
                        .iter()
                        .find(|pkg| pkg.bin().bin().name() == name)
                })
                .map(|pkg| {
                    let pkg = pkg.clone();
                    async move { pkg.uninstall().await }
                })
                .map(tokio::spawn)
                .collect::<Vec<_>>() as Vec<JoinHandle<Result<()>>>;
            for job in try_join_all(jobs).await? {
                if let Err(e) = job {
                    warn!("failed to uninstall: {}", e);
                }
            }
            return Ok(());
        }

        Ok(())
    }

    pub async fn check(&self) -> Result<()> {
        todo!()
    }

    pub async fn install(&self) -> Result<()> {
        let task = |pkg: BinaryPackage| async move {
            if !pkg.has_installed().await {
                pkg.install().await
            } else {
                info!("installed bin {} is skipped", pkg.bin().bin().name());
                Ok::<_, Error>(())
            }
        };

        let jobs = self
            .bin_pkgs
            .iter()
            .map(Clone::clone)
            .map(task)
            .map(tokio::spawn)
            .collect::<Vec<_>>() as Vec<JoinHandle<Result<()>>>;
        debug!("waiting for install {} jobs", jobs.len());

        let mut fails = 0;
        for job in join_all(jobs).await {
            if let Err(e) = job? {
                error!("failed to install: {}", e);
                fails += 1;
            }
        }
        if fails > 0 {
            bail!("install has {} failed tasks", fails);
        }
        Ok(())
    }
}

async fn unused_bins(mapper: &Mapper, bins: &[Binary]) -> Result<Vec<Binary>> {
    let unused = mapper
        .select_all()
        .await?
        .into_iter()
        .filter(|info| !bins.iter().any(|bin| bin.name() == info.name()))
        .map(|info| {
            BinaryBuilder::default()
                .name(info.name())
                .source(&serde_json::from_str::<Source>(info.source())?)?
                .build()
                .map_err(Into::into)
        })
        .collect::<Result<Vec<_>>>()?;
    debug!(
        "found {} bins of unused and {} bins of used",
        unused.len(),
        bins.len()
    );
    Ok(unused)
}

fn build_client() -> Result<Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_static("application/vnd.github.v3+json"),
    );
    let name = "Authorization";
    if let Ok(val) = std::env::var(name) {
        info!("loaded token {} for github rate limit", name);
        headers.insert(name, header::HeaderValue::from_str(&val)?);
    }
    headers.insert(header::USER_AGENT, header::HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/100.0.4896.88 Safari/537.36"));

    ClientBuilder::new()
        .default_headers(headers)
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(Into::into)
}

async fn build_mapper(p: impl AsRef<Path>) -> Result<Mapper> {
    let p = p.as_ref();

    let url = format!("sqlite:{}", p.display());
    let mut opts = SqlitePoolOptions::new().max_connections((num_cpus::get() + 2) as u32);

    if afs::metadata(p).await.is_err() {
        if let Some(p) = p.parent() {
            if afs::metadata(p).await.is_err() {
                trace!("creating all dirs for sqlite db {}", p.display());
                create_dir_all(p).await?;
            }
        }
        trace!("creating db file: {}", p.display());
        afs::File::create(p).await?;

        let init_sql = include_str!("../schema.sql");

        opts = opts.after_connect(move |con| {
            Box::pin(async move {
                trace!("executing sql for init sqlite: {}", init_sql);
                let mut rows = con.execute_many(init_sql);
                while let Some(row) = rows.next().await {
                    trace!("get row: {:?}", row?);
                }
                Ok(())
            })
        });
    }
    debug!("connecting sqlite db for {}", url);
    let pool = opts.connect(&url).await?;

    Ok(Mapper { pool })
}
