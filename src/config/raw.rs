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
#[serde(default, rename_all = "kebab-case")]
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
    use users::User;

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

    #[test]
    fn test_commands() -> Result<()> {
        use std::os::unix::process::CommandExt;
        use std::process::*;

        let f = |id| {
            let out = Command::new("sh")
                .env_clear()
                .arg("-c")
                .arg("id; echo $HOME")
                .uid(id)
                .gid(id)
                .spawn()?
                .wait_with_output()?;
            println!(
                "user id: {}\nstdout: {}\nstderr: {}\nstatus: {}",
                id,
                String::from_utf8(out.stdout)?,
                String::from_utf8(out.stderr)?,
                out.status
            );
            Ok::<_, Error>(())
        };

        f(0)?;
        f(1000)?;
        f(0)?;
        f(1000)?;

        Ok(())
    }
}

mod a {
    use super::*;

    // new feature
    #[derive(Debug, PartialEq, Eq, Default, Getters, Serialize, Deserialize)]
    #[serde(default, rename_all = "kebab-case")]
    struct Config {
        pub bins: Vec<Binary>,
    }

    #[derive(Debug, PartialEq, Eq, Default, Getters, Serialize, Deserialize)]
    #[serde(default, rename_all = "kebab-case")]
    struct Binary {
        pick: Option<Vec<String>>,

        #[serde(rename = "hook")]
        hooks: Option<Vec<Hook>>,

        #[serde(rename = "bin")]
        bins: Option<IndexMap<String, BinIn>>,

        #[serde(flatten)]
        git: Git,

        snippet: Option<Snippet>,
    }

    #[derive(Debug, PartialEq, Eq, Default, Getters, Serialize, Deserialize)]
    #[serde(default, rename_all = "kebab-case")]
    struct Snippet {
        commands: Option<Vec<String>>,
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
        work_dir: Option<String>,

        /// to execute command
        shebang: Option<String>,

        /// to do hook
        command: String,

        /// when to execute
        #[serde(rename = "on")]
        ons: Vec<HookOn>,

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
        pick: Option<Vec<String>>,

        #[serde(rename = "type")]
        ty: CompletionType,
    }

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    enum CompletionType {
        Fpath,
        Source,
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
snippet.commands = ['python3 /a/b.py', 'python3 /a/c.py']
snippet.urls = ['https://a.com/a', 'https://a.com/b']
bin.mvnup.pick = 'a/b'


[[bins]]
github = "gohugoio/hugo"
release = true
pick = ['.*extended.*Linux.*tar.*']
bin.hugo.pick = '*hugo*'
[bins.hugo.completions._hugo]
type = 'fpath'
command = 'hugo completions zsh'

"#;
        let config: Config = toml::from_str(s)?;
        println!("{:?}", config);
        assert_eq!(config.bins.len(), 2);

        let bin = &config.bins[0];
        assert!(bin.snippet.is_none());
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
            assert_eq!(hooks[0].user.as_deref(), Some("root"));
            assert_eq!(&hooks[0].ons, &[HookOn::Install]);
            assert_eq!(hooks[0].shebang.as_deref(), Some("/bin/sh -c"));
            assert_eq!(hooks[0].command, "echo test > a");
            assert_eq!(hooks[0].work_dir, None);

            assert_eq!(hooks[1].user.as_deref(), Some("root"));
            assert_eq!(&hooks[1].ons, &[HookOn::Uninstall]);
            assert_eq!(hooks[1].shebang, None);
            assert_eq!(hooks[1].command, "rm -rf a");
            assert_eq!(hooks[1].work_dir.as_deref(), Some("/usr/local/bin"));

            assert_eq!(hooks[2].user.as_deref(), Some("root"));
            assert_eq!(
                &hooks[2].ons,
                &[HookOn::Update, HookOn::Install, HookOn::Uninstall]
            );
            assert_eq!(hooks[2].shebang, None);
            assert_eq!(hooks[2].command, "systemctl daemon-reload");
            assert_eq!(hooks[2].work_dir, None);
        }

        let bin = &config.bins[1];
        // assert!(bin.git.is_none());
        {
            assert!(bin.snippet.is_some());
            assert_eq!(
                bin.snippet.as_ref().and_then(|a| a.commands.as_ref()),
                Some(
                    &["python3 /a/b.py", "python3 /a/c.py"]
                        .into_iter()
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                )
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

        Ok(())
    }
}
