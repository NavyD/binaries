use std::fmt;

use getset::Getters;
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, PartialEq, Eq, Default, Getters, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct Config {
    pub bins: Vec<Binary>,

    pub default: Option<DefaultConfig>,
}

#[derive(Debug, PartialEq, Eq, Default, Getters, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct DefaultConfig {
    bin: Option<BinIn>,

    completion: Option<Completion>,
}

#[derive(Debug, PartialEq, Eq, Default, Getters, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct Binary {
    /// 对git文件和snippet url选择出合适的。支持正则，允许pick下载多个
    pick: Option<Vec<String>>,

    #[serde(rename = "hook")]
    hooks: Option<Vec<Hook>>,

    #[serde(rename = "bin")]
    bins: Option<IndexMap<String, BinIn>>,

    #[serde(flatten)]
    git: Git,

    #[serde(flatten)]
    snippet: Option<Snippet>,

    completion: Option<Completion>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
struct Snippet {
    #[serde(rename = "snippet-command")]
    command: Option<Command>,

    #[serde(rename = "snippets")]
    urls: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
struct Git {
    github: Option<String>,

    #[serde(default)]
    release: bool,

    #[serde(default)]
    prerelease: bool,

    #[serde(flatten)]
    reference: Option<GitReference>,
}

/// A Git reference.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitReference {
    /// From the tip of a branch.
    Branch(String),
    /// From a specific revision.
    Rev(String),
    /// From a tag.
    Tag(String),
}

/// Indicates a bin to be installed
#[derive(Debug, PartialEq, Eq, Default, Getters, Serialize, Deserialize)]
struct BinIn {
    /// Select a bin installation from the downloaded files
    pick: Option<String>,

    /// Bin installation mode
    #[serde(flatten)]
    ty: Option<BinInType>,

    /// Path to install bin
    path: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum BinInType {
    Shim { template: Option<String> },
    Link,
    Symlink,
    Copy,
}

#[derive(Debug, PartialEq, Eq, Default, Getters, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct Hook {
    #[serde(flatten)]
    command: Command,

    /// when to execute
    #[serde(rename = "on")]
    ons: Vec<HookOn>,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize)]
#[serde(default, rename_all = "kebab-case")]
struct Command {
    /// to do hook
    value: String,

    work_dir: Option<String>,

    /// to execute command
    shebang: Option<String>,

    /// the user who executed the command
    user: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum HookOn {
    Install,
    Update,
    Uninstall,
    Extract,
}

#[derive(Debug, PartialEq, Eq, Getters, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Completion {
    fpath: Option<Vec<String>>,
    source: Option<Vec<String>>,
}

mod command {
    use std::result;

    use serde::de;

    use super::*;

    struct Visitor;

    #[derive(Deserialize)]
    #[serde(rename_all = "kebab-case")]
    struct CommandAux {
        work_dir: Option<String>,

        /// to execute command
        shebang: Option<String>,

        /// to do hook
        value: String,

        /// the user who executed the command
        user: Option<String>,
    }

    impl From<CommandAux> for Command {
        fn from(aux: CommandAux) -> Self {
            let CommandAux {
                work_dir,
                shebang,
                value,
                user,
            } = aux;

            Self {
                value,
                shebang,
                user,
                work_dir,
            }
        }
    }

    impl From<&str> for Command {
        fn from(s: &str) -> Self {
            Self {
                value: s.to_owned(),
                ..Default::default()
            }
        }
    }

    impl<'de> de::Visitor<'de> for Visitor {
        type Value = Command;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> result::Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(From::from(value))
        }

        fn visit_map<M>(self, visitor: M) -> result::Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            let aux: CommandAux =
                Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))?;
            Ok(aux.into())
        }
    }

    impl<'de> Deserialize<'de> for Command {
        fn deserialize<D>(deserializer: D) -> result::Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(Visitor)
        }
    }

    #[cfg(test)]
    mod tests {
        use anyhow::Result;

        use super::*;

        #[derive(Debug, Deserialize)]
        struct CommandCxt {
            command: Option<Command>,
        }

        #[test]
        fn test_de() -> Result<()> {
            let val = "echo 'test'";
            let s = format!(r#"command = "{val}""#);
            let cmd = toml::from_str::<CommandCxt>(&s)?;
            // assert!(cmd.command.is_some());
            let cmd = cmd.command.unwrap();
            assert_eq!(cmd.value, val);
            assert_eq!(cmd.user, None);
            assert_eq!(cmd.work_dir, None);
            assert_eq!(cmd.shebang, None);

            let s = format!(
                r#"
[command]
value = "{val}"
work-dir = 'a'
user = 'root'
shebang = 'sh -c'
            "#
            );
            let cmd = toml::from_str::<CommandCxt>(&s)?;
            assert!(cmd.command.is_some());
            let cmd = cmd.command.unwrap();
            assert_eq!(cmd.value, val);
            assert_eq!(cmd.user.as_deref(), Some("root"));
            assert_eq!(cmd.work_dir.as_deref(), Some("a"));
            assert_eq!(cmd.shebang.as_deref(), Some("sh -c"));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::*;

    #[test]
    fn test_a() -> Result<()> {
        let s = r#"
[[bins]]
github = "Dreamacro/clash"
release = true
tag = 'premium'
pick = ['clash-{{os}}-{{arch}}.*.gz']"#;
        let config: Config = toml::from_str(s)?;
        println!("{:?}", config);
        Ok(())
    }

    #[test]
    fn test_raw() -> Result<()> {
        let s = r#"
[[bins]]
github = "Dreamacro/clash"
release = true
tag = 'premium'
pick = ['clash-{{os}}-{{arch}}.*.gz']

[bins.bin.clash]
pick = 'clash*'
path = '/usr/local/bin'
type = 'copy'

[[bins.hook]]
user = 'root'
on = ['install']
shebang = '/bin/sh -c'
command = 'echo test > a'

[[bins.hook]]
work-dir = '/usr/local/bin'
user = 'root'
on = ['uninstall']
command = 'rm -rf a'

[[bins.hook]]
user = 'root'
on = ['update', 'install', 'uninstall']
command = 'systemctl daemon-reload'


[[bins]]
pick = ['b']
snippet-command = 'python3 /a/b.py'
snippet.urls = ['https://a.com/a', 'https://a.com/b']
bin.mvnup.pick = 'a/b'


[[bins]]
github = "gohugoio/hugo"
release = true
pick = ['.*extended.*Linux.*tar.*']
bin.hugo.pick = '*hugo*'
[bins.completion]
fpath = ['_*']
source = ['.*.zsh']
    "#;
        let config: Config = toml::from_str(s)?;
        println!("{:?}", config);
        assert_eq!(config.bins.len(), 3);

        let bin = &config.bins[0];
        {
            let git = &bin.git;
            assert_eq!(git.github.as_deref(), Some("Dreamacro/clash"));
            assert!(git.release);
            assert_eq!(git.reference, Some(GitReference::Tag("premium".to_owned())));
        }

        assert_eq!(bin.pick.as_ref().map(|a| a.len()), Some(1));
        assert_eq!(
            bin.pick.as_ref().map(|a| &*a[0]),
            Some("clash-{{os}}-{{arch}}.*.gz")
        );

        {
            assert_eq!(bin.bins.as_ref().map(|a| a.len()), Some(1));
            let binin = bin.bins.as_ref().map(|a| &a["clash"]);
            assert_eq!(binin.unwrap().pick.as_deref(), Some("clash*"));
            assert_eq!(binin.unwrap().path.as_deref(), Some("/usr/local/bin"));
            assert_eq!(binin.unwrap().ty.as_ref(), Some(&BinInType::Copy));
        }

        {
            assert_eq!(bin.hooks.as_ref().map(Vec::len), Some(3));
            let hooks = bin.hooks.as_ref().unwrap();
            assert_eq!(&hooks[0].ons, &[HookOn::Install]);
            assert_eq!(hooks[0].command.user.as_deref(), Some("root"));
            assert_eq!(hooks[0].command.shebang.as_deref(), Some("/bin/sh -c"));
            assert_eq!(hooks[0].command.value, "echo test > a");
            assert_eq!(hooks[0].command.work_dir, None);

            assert_eq!(&hooks[1].ons, &[HookOn::Uninstall]);
            assert_eq!(hooks[1].command.user.as_deref(), Some("root"));
            assert_eq!(hooks[1].command.shebang, None);
            assert_eq!(hooks[1].command.value, "rm -rf a");
            assert_eq!(hooks[1].command.work_dir.as_deref(), Some("/usr/local/bin"));

            assert_eq!(
                &hooks[2].ons,
                &[HookOn::Update, HookOn::Install, HookOn::Uninstall]
            );
            assert_eq!(hooks[2].command.user.as_deref(), Some("root"));
            assert_eq!(hooks[2].command.shebang, None);
            assert_eq!(hooks[2].command.value, "systemctl daemon-reload");
            assert_eq!(hooks[2].command.work_dir, None);
        }

        let bin = &config.bins[1];
        // assert!(bin.git.is_none());
        {
            assert_eq!(
                bin.snippet.as_ref().and_then(|a| a.command.as_ref()),
                Some(&Command {
                    value: "".to_owned(),
                    ..Default::default()
                })
            );
            assert_eq!(
                bin.snippet.as_ref().and_then(|a| a.urls.as_ref()),
                Some(
                    &["https://a.com/a", "https://a.com/b"]
                        .into_iter()
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                )
            );

            let binin = bin.bins.as_ref().and_then(|a| a.get("mvnup"));
            assert_eq!(binin.and_then(|a| a.pick.as_deref()), Some("a/b"))
        }

        let bin = &config.bins[2];
        {
            let git = &bin.git;
            assert_eq!(git.github.as_deref(), Some("gohugoio/hugo"));
            assert!(git.release);
            assert_eq!(git.reference, None);
        }

        {
            let cmpl = bin.completion.as_ref();
            assert_eq!(
                cmpl.and_then(|a| a.fpath.as_ref()),
                Some(
                    &["_*"]
                        .into_iter()
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                )
            );
            assert_eq!(
                cmpl.and_then(|a| a.source.as_ref()),
                Some(
                    &[".*.zsh"]
                        .into_iter()
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                )
            );
        }

        Ok(())
    }
}
