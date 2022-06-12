use std::{
    fmt,
    ops::{Deref, DerefMut},
};

use getset::Getters;
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, PartialEq, Eq, Getters, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Config {
    pub bins: Vec<Binary>,

    pub default: Option<DefaultConfig>,

    pub locals: Option<IndexMap<String, Local>>,
}

#[derive(Debug, PartialEq, Eq, Getters, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Local {
    hooks: Vec<Hook>,
}

#[derive(Debug, PartialEq, Eq, Getters, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct DefaultConfig {
    exe: Option<Exe>,

    completion: Option<Completion>,
}

#[derive(Debug, PartialEq, Eq, Getters, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Binary {
    /// 对git文件和snippet url选择出合适的。支持正则，允许pick下载多个
    picks: Option<Vec<String>>,

    hooks: Option<Vec<Hook>>,

    exes: Option<IndexMap<String, Exe>>,

    #[serde(flatten)]
    source: Source,

    completion: Option<Completion>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
// #[serde(tag = "source", rename_all = "kebab-case")]
#[serde(untagged, rename_all = "kebab-case")]
enum Source {
    Local {
        local: String,
    },
    Git {
        #[serde(flatten)]
        source: GitSource,

        #[serde(default)]
        release: bool,

        #[serde(default)]
        prerelease: bool,

        #[serde(flatten)]
        reference: Option<GitReference>,

        picks: Option<Vec<String>>,
    },
    Snippet {
        command: Option<Command>,

        urls: Option<Vec<String>>,

        picks: Option<Vec<String>>,

        check: Option<SnippetCheck>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Default)]
pub struct Template(String);

impl DerefMut for Template {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Deref for Template {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
struct SnippetCheck {
    url: Option<Template>,
    command: Option<Command>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
struct Snippet {
    command: Option<Command>,

    urls: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum GitSource {
    Github(String),
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
#[derive(Debug, PartialEq, Eq, Getters, Serialize, Deserialize)]
struct Exe {
    /// Select a bin installation from the downloaded files
    pick: Option<String>,

    /// Bin installation mode
    #[serde(flatten)]
    ty: Option<ExeType>,

    /// Path to install bin
    path: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum ExeType {
    Shim {
        #[serde(default = "defaults::default_exe_type_template")]
        template: String,
    },
    Link,
    Symlink,
    Copy,
}

mod defaults {
    pub fn default_exe_type_template() -> String {
        r#"#!/usr/bin/env sh
"{{bins.exe_dir}}/{{name}}" "$@"
"#
        .to_string()
    }
}

#[derive(Debug, PartialEq, Eq, Getters, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Hook {
    #[serde(flatten)]
    command: Command,

    /// when to execute
    #[serde(rename = "on")]
    ons: Vec<HookOn>,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
struct Command {
    /// to do hook
    #[serde(rename = "command")]
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

enum Completions {
    Fpath { paths: Vec<String>, mv: bool },
    Source { paths: Vec<String> },
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
        #[serde(rename = "command")]
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
command = "{val}"
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
picks = ['clash-{{os}}-{{arch}}.*.gz']"#;
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
picks = ['clash-{{os}}-{{arch}}.*.gz']

[bins.exes.clash]
pick = 'clash*'
path = '/usr/local/bin'
type = 'copy'

[[bins.hooks]]
user = 'root'
on = ['install']
shebang = '/bin/sh -c'
command = 'echo test > a'

[[bins.hooks]]
work-dir = '/usr/local/bin'
user = 'root'
on = ['uninstall']
command = 'rm -rf a'

[[bins.hooks]]
user = 'root'
on = ['update', 'install', 'uninstall']
command = 'systemctl daemon-reload'


[[bins]]
picks = ['b']
command = 'python3 /a/b.py'
urls = ['https://a.com/a', 'https://a.com/b']
exes.mvnup.pick = 'a/b'


[[bins]]
github = "gohugoio/hugo"
release = true
picks = ['.*extended.*Linux.*tar.*']
exes.hugo.pick = '*hugo*'

[bins.completion]
fpath = ['_*']
source = ['.*.zsh']
    "#;
        let config: Config = toml::from_str(s)?;
        println!("{:?}", config);
        assert_eq!(config.bins.len(), 3);

        let bin = &config.bins[0];
        {
            assert!(matches!(bin.source, Source::Git { .. }));
            if let Source::Git {
                source,
                release,
                prerelease,
                reference,
                picks,
            } = bin.source.clone()
            {
                assert_eq!(source, GitSource::Github("Dreamacro/clash".to_owned()));
                assert!(release);
                assert!(!prerelease);
                assert_eq!(reference, Some(GitReference::Tag("premium".to_owned())));
            }
        }

        // assert_eq!(bin.picks.as_ref().map(|a| a.len()), Some(1));
        // assert_eq!(
        //     bin.picks.as_ref().map(|a| &*a[0]),
        //     Some("clash-{{os}}-{{arch}}.*.gz")
        // );

        // {
        //     assert_eq!(bin.exes.as_ref().map(|a| a.len()), Some(1));
        //     let binin = bin.exes.as_ref().map(|a| &a["clash"]);
        //     assert_eq!(binin.unwrap().pick.as_deref(), Some("clash*"));
        //     assert_eq!(binin.unwrap().path.as_deref(), Some("/usr/local/bin"));
        //     assert_eq!(binin.unwrap().ty.as_ref(), Some(&ExeType::Copy));
        // }

        // {
        //     assert_eq!(bin.hooks.as_ref().map(Vec::len), Some(3));
        //     let hooks = bin.hooks.as_ref().unwrap();
        //     assert_eq!(&hooks[0].ons, &[HookOn::Install]);
        //     assert_eq!(hooks[0].command.user.as_deref(), Some("root"));
        //     assert_eq!(hooks[0].command.shebang.as_deref(), Some("/bin/sh -c"));
        //     assert_eq!(hooks[0].command.value, "echo test > a");
        //     assert_eq!(hooks[0].command.work_dir, None);

        //     assert_eq!(&hooks[1].ons, &[HookOn::Uninstall]);
        //     assert_eq!(hooks[1].command.user.as_deref(), Some("root"));
        //     assert_eq!(hooks[1].command.shebang, None);
        //     assert_eq!(hooks[1].command.value, "rm -rf a");
        //     assert_eq!(hooks[1].command.work_dir.as_deref(), Some("/usr/local/bin"));

        //     assert_eq!(
        //         &hooks[2].ons,
        //         &[HookOn::Update, HookOn::Install, HookOn::Uninstall]
        //     );
        //     assert_eq!(hooks[2].command.user.as_deref(), Some("root"));
        //     assert_eq!(hooks[2].command.shebang, None);
        //     assert_eq!(hooks[2].command.value, "systemctl daemon-reload");
        //     assert_eq!(hooks[2].command.work_dir, None);
        // }

        // let bin = &config.bins[1];
        // assert!(bin.git.is_none());
        // {
        //     assert_eq!(
        //         bin.snippet.as_ref().and_then(|a| a.command.as_ref()),
        //         Some(&Command {
        //             value: "python3 /a/b.py".to_owned(),
        //             ..Default::default()
        //         })
        //     );
        //     assert_eq!(
        //         bin.snippet.as_ref().and_then(|a| a.urls.as_ref()),
        //         Some(
        //             &["https://a.com/a", "https://a.com/b"]
        //                 .into_iter()
        //                 .map(ToOwned::to_owned)
        //                 .collect::<Vec<_>>()
        //         )
        //     );

        //     let binin = bin.exes.as_ref().and_then(|a| a.get("mvnup"));
        //     assert_eq!(binin.and_then(|a| a.pick.as_deref()), Some("a/b"))
        // }

        // let bin = &config.bins[2];
        // {
        //     assert!(bin.git.is_some());
        //     let git = bin.git.as_ref().unwrap();
        //     assert_eq!(git.source, GitSource::Github("gohugoio/hugo".to_owned()));
        //     assert!(git.release);
        //     assert_eq!(git.reference, None);
        // }

        // {
        //     let cmpl = bin.completion.as_ref();
        //     assert_eq!(
        //         cmpl.and_then(|a| a.fpath.as_ref()),
        //         Some(
        //             &["_*"]
        //                 .into_iter()
        //                 .map(ToOwned::to_owned)
        //                 .collect::<Vec<_>>()
        //         )
        //     );
        //     assert_eq!(
        //         cmpl.and_then(|a| a.source.as_ref()),
        //         Some(
        //             &[".*.zsh"]
        //                 .into_iter()
        //                 .map(ToOwned::to_owned)
        //                 .collect::<Vec<_>>()
        //         )
        //     );
        // }

        Ok(())
    }
}

#[cfg(test)]
mod tests11 {
    use super::*;

    #[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]

    struct A {
        #[serde(flatten)]
        source: Source,
    }

    #[test]
    fn test_name() {
        let s = r#"
command = 'a'
#github = 'ttess'
"#;
        let config: A = toml::from_str(s).unwrap();
        println!("{:?}", config);
        // assert!(config.bins[0].git.is_some());
        // assert!(config.bins[1].git.is_none());
    }
}
