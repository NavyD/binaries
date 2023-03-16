use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
// #[serde(tag = "source", rename_all = "kebab-case")]
#[serde(untagged, rename_all = "kebab-case")]
enum Source {
    Local {
        local: String,
    },
    Git {
        #[serde(flatten)]
        source: String,

        #[serde(flatten)]
        reference: Option<GitReference>,

        picks: Option<Vec<String>>,
    },
    Snippet(Snippet),
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]

enum Snippet {
    Urls(Vec<String>),
    Command(Command),
    GithubRelease {

        prerelease: bool,

        tag: Option<String>,

        picks: Option<Vec<String>>,
    },
}

/// A GitHub repository identifier.
#[derive(Debug, PartialEq, Clone, Eq)]
pub struct GitHubRepository {
    /// The GitHub username / organization.
    pub owner: String,
    /// The GitHub repository name.
    pub name: String,
}


/// A Git reference.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitReference {
    /// From the tip of a branch.
    Branch(String),
    /// From a specific revision.
    Rev(String),
    /// From a tag.
    Tag(String),
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
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

mod command {
    use std::{fmt, result};

    use serde::{de, Deserializer};

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
