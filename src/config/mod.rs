use std::{fs::read_to_string, path::Path, str::FromStr};

use anyhow::{bail, Error, Result};
use derive_builder::Builder;
use getset::{Getters, Setters};
use log::{debug, trace};
use serde::{Deserialize, Serialize};

use self::raw::RawConfig;

pub mod raw;
mod file;

#[derive(Debug, Getters, Setters, Clone, Builder)]
#[getset(get = "pub")]
#[builder(pattern = "mutable", setter(into, strip_option))]
pub struct Config {
    bins: Vec<Binary>,
}

#[derive(Debug, Getters, Setters, Clone, Builder)]
#[getset(get = "pub", set)]
#[builder(pattern = "mutable", setter(into, strip_option))]
pub struct Binary {
    #[builder(default)]
    name: String,

    #[builder(default)]
    version: Option<String>,

    #[builder(default)]
    hook: Option<HookAction>,

    /// a glob of executable file in zip. for help to comfirm exe bin
    #[builder(default)]
    bin_glob: Option<String>,

    #[builder(default)]
    pick_regex: Option<String>,

    #[builder(setter(custom))]
    source: Source,
}

impl BinaryBuilder {
    pub fn source<T>(&mut self, source: T) -> Result<&mut Self>
    where
        T: TryInto<Source, Error = Error>,
    {
        self.source.replace(source.try_into()?);
        Ok(self)
    }
}

#[derive(
    Debug, Default, PartialEq, Eq, Getters, Setters, Clone, Builder, Serialize, Deserialize,
)]
#[getset(get = "pub", set)]
#[builder(pattern = "mutable", setter(into, strip_option))]
pub struct HookAction {
    #[builder(default)]
    install: Option<String>,
    #[builder(default)]
    update: Option<String>,
    #[builder(default)]
    extract: Option<String>,
    #[builder(default)]
    uninstall: Option<String>,
}

/// A GitHub repository identifier.
#[derive(Debug, PartialEq, Clone, Eq)]
pub struct GitHubRepository {
    /// The GitHub username / organization.
    pub owner: String,
    /// The GitHub repository name.
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Github { owner: String, repo: String },
}

impl FromStr for Source {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        trace!("parsing Source from str: {}", s);
        const DELIMITER: char = ':';
        let a = s.split(DELIMITER).collect::<Vec<_>>();
        if a.len() != 2 {
            bail!("failed to parse Source: then len {} is not 2", a.len());
        }
        let (name, value) = (a[0].trim().to_lowercase(), a[1].trim());
        match name.as_str() {
            "github" => {
                let delimiter = '/';
                let v = value.split(delimiter).collect::<Vec<_>>();
                if v.len() != 2 {
                    bail!(
                        "source parse error: splits {} is not 2 by delimiter {}",
                        v.len(),
                        delimiter
                    );
                }
                Ok(Source::Github {
                    owner: v[0].to_owned(),
                    repo: v[1].to_owned(),
                })
            }
            _ => bail!("unsupported name: {}", name),
        }
    }
}

impl TryFrom<&str> for Source {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<&Source> for Source {
    type Error = Error;

    fn try_from(value: &Source) -> Result<Self, Self::Error> {
        Ok(value.clone())
    }
}

impl TryFrom<RawConfig> for Config {
    type Error = Error;

    fn try_from(raw: RawConfig) -> Result<Self, Self::Error> {
        let bins = raw
            .bins
            .into_iter()
            .map(|(name, bin)| {
                let source = match bin.github() {
                    Some(g) => Source::Github {
                        owner: g.owner.to_owned(),
                        repo: g.name.to_owned(),
                    },
                    None => bail!("not found source"),
                };
                Ok(Binary {
                    bin_glob: bin.bin_glob().as_ref().or(raw.bin_glob.as_ref()).cloned(),
                    hook: bin.hook().as_ref().or(raw.hook.as_ref()).cloned(),
                    name,
                    pick_regex: bin
                        .pick_regex()
                        .as_ref()
                        .or(raw.pick_regex.as_ref())
                        .cloned(),
                    source,
                    version: bin.version().clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Config { bins })
    }
}

pub fn from_path(path: impl AsRef<Path>) -> Result<Config> {
    debug!("loading config from {}", path.as_ref().display());
    let contents = read_to_string(path)?;
    trace!("loaded raw config content: {}", contents);
    let raw: RawConfig = toml::from_str(&contents)?;
    trace!("parsing raw config: {:?}", raw);
    raw.try_into().map_err(Into::into)
}
