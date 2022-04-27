pub mod raw;

use derive_builder::Builder;
use getset::{Getters, Setters};
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

    #[builder(default)]
    pick_regex: Option<String>,

    /// a glob of executable file in zip. for help to comfirm exe bin
    #[builder(default)]
    exe_glob: Option<String>,

    source: Source,
}

impl Binary {
    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or(match &self.source {
            Source::Github { owner: _, repo } => repo.as_str(),
        })
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Source {
    Github { owner: String, repo: String },
}