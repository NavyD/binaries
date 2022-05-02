use std::{fmt, str::FromStr};

use anyhow::anyhow;
use anyhow::{Error, Result};
use getset::Getters;
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{GitHubRepository, HookAction};

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "kebab-case")]
pub struct RawConfig {
    pub bins: IndexMap<String, RawBinary>,

    pub bin_glob: Option<String>,

    pub pick_regex: Option<String>,

    pub hook: Option<HookAction>,
}

#[derive(Debug, PartialEq, Eq, Default, Getters, Serialize, Deserialize)]
#[getset(get = "pub", get_mut = "pub")]
#[serde(default)]
pub struct RawBinary {
    version: Option<String>,

    hook: Option<HookAction>,

    /// a glob of executable file in zip. for help to comfirm exe bin
    bin_glob: Option<String>,

    pick_regex: Option<String>,

    github: Option<GitHubRepository>,
}

impl FromStr for GitHubRepository {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = regex::Regex::new("^(?P<owner>[a-zA-Z0-9_-]+)/(?P<name>[a-zA-Z0-9\\._-]+)$")?;
        let captures = re.captures(s).ok_or_else(|| anyhow!("{}", s))?;
        let owner = captures.name("owner").unwrap().as_str().to_string();
        let name = captures.name("name").unwrap().as_str().to_string();
        Ok(Self { owner, name })
    }
}

impl fmt::Display for GitHubRepository {
    /// Displays as "{owner}/{repository}".
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

macro_rules! impl_serialize_as_str {
    ($name:ident) => {
        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }
    };
}

impl_serialize_as_str! { GitHubRepository }

macro_rules! impl_deserialize_from_str {
    ($module:ident, $name:ident, $expecting:expr) => {
        mod $module {
            use super::*;
            use serde::de;
            use std::result;

            struct Visitor;

            impl<'de> de::Visitor<'de> for Visitor {
                type Value = $name;

                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str($expecting)
                }

                fn visit_str<E>(self, value: &str) -> result::Result<Self::Value, E>
                where
                    E: de::Error,
                {
                    $name::from_str(value).map_err(|e| de::Error::custom(e.to_string()))
                }
            }

            impl<'de> Deserialize<'de> for $name {
                fn deserialize<D>(deserializer: D) -> result::Result<Self, D::Error>
                where
                    D: Deserializer<'de>,
                {
                    deserializer.deserialize_str(Visitor)
                }
            }
        }
    };
}

impl_deserialize_from_str! { github_repository, GitHubRepository, "a GitHub repository" }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ser() -> Result<()> {
        let config = RawConfig {
            bin_glob: Some("{{bin}}".to_owned()),
            pick_regex: Some("{{a}}".to_owned()),
            hook: Some(HookAction {
                extract: Some("a".to_owned()),
                ..Default::default()
            }),
            bins: [
                (
                    "clash",
                    RawBinary {
                        github: "a/b".parse::<GitHubRepository>().ok(),
                        hook: Some(HookAction {
                            install: Some("echo a".to_owned()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                ),
                (
                    "b",
                    RawBinary {
                        github: "c/d".parse::<GitHubRepository>().ok(),
                        ..Default::default()
                    },
                ),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect::<IndexMap<_, _>>(),
        };
        let s = format!(
            r#"
bin-glob = "{bin_glob}"
pick-regex = "{pick_regex}"

[hook]
#install = "install"
extract = "{extract}"

[bins.{name1}]
github = "{github1}"
hook.install = "{install1}"

[bins.{name2}]
github = "{github2}"
"#,
            bin_glob = config.bin_glob.as_ref().unwrap(),
            pick_regex = config.pick_regex.as_ref().unwrap(),
            extract = config
                .hook
                .as_ref()
                .and_then(|h| h.extract().as_ref())
                .unwrap(),
            name1 = config.bins.iter().next().unwrap().0,
            github1 = config
                .bins
                .iter()
                .next()
                .unwrap()
                .1
                .github()
                .as_ref()
                .unwrap(),
            install1 = config
                .bins
                .iter()
                .next()
                .unwrap()
                .1
                .hook
                .as_ref()
                .unwrap()
                .install
                .as_ref()
                .unwrap(),
            name2 = config.bins.iter().nth(1).unwrap().0,
            github2 = config
                .bins
                .iter()
                .nth(1)
                .unwrap()
                .1
                .github()
                .as_ref()
                .unwrap(),
        );
        let raw: RawConfig = toml::from_str(&s)?;
        assert_eq!(config, raw);
        Ok(())
    }
}
