use std::{
    path::{Path, PathBuf},
    process::exit,
    time::Duration,
};

use anyhow::{anyhow, bail, Error, Result};
use binaries::{
    config::Config,
    manager::{BinaryPackage, BinaryPackageBuilder},
    updated_info::Mapper,
    CRATE_NAME,
};
use clap::{ArgEnum, Args, Parser, Subcommand};
use directories::{BaseDirs, ProjectDirs};
use futures_util::future::{join_all, try_join_all};
use handlebars::Handlebars;
use log::{debug, info, trace, warn};
use once_cell::sync::Lazy;
use reqwest::{
    header::{self, HeaderMap},
    Client, ClientBuilder,
};
use sqlx::sqlite::SqlitePoolOptions;
use strum::EnumString;
use tokio::{fs as afs, task::JoinHandle};

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
            Commands::Install => {
                pm.install().await?;
            }
            Commands::Uninstall(args) => {}
            _ => {}
        }
        todo!()
    }

    async fn load_config(&self) -> Result<Config> {
        let path = self
            .config_path
            .as_deref()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| PROJECT_DIRS.config_dir().join("config.toml"));

        info!("loading config from {}", path.display());
        let config = afs::read_to_string(path).await?;
        trace!("loaded config str: {}", config);
        toml::from_str(&config).map_err(Into::into)
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
    r#type: Option<UninstallType>,
}

#[derive(Debug, ArgEnum, Clone, EnumString)]
enum UninstallType {
    All,
    Unused,
    Used,
}

static TEMPLATE: Lazy<Handlebars<'static>> = Lazy::new(Handlebars::new);

#[derive(Debug, Clone)]
pub struct PackageManager {
    bin_pkgs: Vec<BinaryPackage>,
    config: Config,
    // client: Client,
    // mapper: Mapper,
    // project_dirs: ProjectDirs,
    // base_dirs: BaseDirs,
}

impl PackageManager {
    pub async fn new(config: Config) -> Result<Self> {
        let project_dirs = ProjectDirs::from("xyz", "navyd", CRATE_NAME)
            .ok_or_else(|| anyhow!("no project dirs"))?;
        let base_dirs = BaseDirs::new().ok_or_else(|| anyhow!("no base dirs"))?;

        let client = build_client()?;
        let mapper =
            build_mapper(project_dirs.data_dir().join(&format!("{}.db", CRATE_NAME))).await?;

        let build_fn = |bin| {
            let (data_dir, cache_dir, executable_dir) = (
                project_dirs.data_dir(),
                project_dirs.cache_dir(),
                base_dirs.executable_dir(),
            );
            let client = client.clone();
            let mapper = mapper.clone();
            async move {
                BinaryPackageBuilder::default()
                    .bin(bin)
                    .data_dir(data_dir.to_owned())
                    .link_path(
                        executable_dir
                            .map(ToOwned::to_owned)
                            .ok_or_else(|| anyhow!("no exe dir"))?,
                    )
                    .cache_dir(cache_dir.to_owned())
                    .client(client)
                    .mapper(mapper)
                    .build()
                    .await
            }
        };

        let bin_pkgs: Vec<_> =
            try_join_all(config.bins().iter().map(Clone::clone).map(build_fn)).await?;
        trace!("got {} bin packages", bin_pkgs.len());

        // check db and config file
        for info in mapper.select_all().await? {
            if !config.bins().iter().all(|bin| bin.name() == info.name()) {
                warn!(
                    "found a bin {} in db but not configured. create time: {}",
                    info.name(),
                    info.create_time()
                );
            }
        }

        Ok(Self { bin_pkgs, config })
    }

    pub async fn uninstall(&self, args: &UninstallArgs) -> Result<()> {
        if let Some(names) = &args.names {
            let jobs = names
                .iter()
                .flat_map(|name| self.bin_pkgs.iter().find(|pkg| pkg.bin().name() == name))
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
        }
        todo!()
    }

    pub async fn check(&self) -> Result<()> {
        todo!()
    }

    pub async fn install(&self) -> Result<()> {
        let task = |pkg: BinaryPackage| async move {
            if !pkg.has_installed().await {
                pkg.install(Some(&*TEMPLATE)).await
            } else {
                info!("installed bin {} is skipped", pkg.bin().name());
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

        for job in join_all(jobs).await {
            if let Err(e) = job? {
                warn!("failed to install: {}", e);
            }
        }
        Ok(())
    }
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
    let pool = SqlitePoolOptions::new()
        .max_connections((num_cpus::get() + 2) as u32)
        .connect(&format!("sqlite:{}", p.as_ref().display()))
        .await?;

    Ok(Mapper { pool })
}
