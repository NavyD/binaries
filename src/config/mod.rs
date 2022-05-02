use std::str::FromStr;

use anyhow::{bail, Error, Result};
use derive_builder::Builder;
use getset::{Getters, Setters};
use log::trace;
use serde::{Deserialize, Serialize};

#[derive(Debug, Getters, Setters, Clone, Builder, Serialize, Deserialize)]
#[getset(get = "pub")]
#[builder(pattern = "mutable", setter(into, strip_option))]
pub struct Config {
    bins: Vec<Binary>,
}

#[derive(Debug, Getters, Setters, Clone, Builder, Serialize, Deserialize)]
#[getset(get = "pub", set)]
#[builder(pattern = "mutable", setter(into, strip_option))]
pub struct Binary {
    #[builder(default)]
    #[getset(skip)]
    name: Option<String>,

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

impl Binary {
    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or(match &self.source {
            Source::Github { owner: _, repo } => repo.as_str(),
        })
    }
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

#[derive(Debug, Getters, Setters, Clone, Builder, Serialize, Deserialize)]
#[getset(get = "pub", set)]
#[builder(pattern = "mutable", setter(into, strip_option))]
pub struct HookAction {
    #[builder(default)]
    install: Option<String>,
    #[builder(default)]
    update: Option<String>,
    #[builder(default)]
    extract: Option<String>,
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

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::*;

    #[test]
    fn test_name() -> Result<()> {
        let bin = BinaryBuilder::default()
            .name("clash")
            .bin_glob("clash")
            .hook(
                HookActionBuilder::default()
                    .extract("tar xvf {{ from }} -C {{to}}")
                    .build()?,
            )
            .source("github:a/b")?
            // .pick_regex("")
            .build()?;
        let config = Config {
            bins: vec![bin.clone(), bin],
        };

        let s = serde_yaml::to_string(&config)?;
        println!("{}", s);

        let source = "github:a/b".parse::<Source>()?;
        println!("{}", serde_json::to_string(&source)?);
        Ok(())
    }

    #[test]
    fn test_config() -> Result<()> {
        let s = r#"
bins:
  - name: clash
    source:
      github:
        owner: a
        repo: b
"#;
        let config: Config = serde_yaml::from_str(s)?;
        assert_eq!(config.bins.len(), 1);
        assert_eq!(config.bins[0].name(), "clash");
        Ok(())
    }
}
