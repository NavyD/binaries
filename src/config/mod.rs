pub mod raw;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use derive_builder::Builder;
use getset::{Getters, Setters};
use serde::{Deserialize, Serialize};

#[derive(Debug, Getters, Setters, Clone, Builder, Serialize, Deserialize)]
#[getset(get = "pub", set, get)]
#[builder(pattern = "mutable", setter(into, strip_option))]
pub struct Config {
    bins: Vec<Binary>,
}

#[derive(Debug, Getters, Setters, Clone, Builder, Serialize, Deserialize)]
#[getset(get = "pub", set, get)]
pub struct Binary {
    #[builder(default)]
    name: String,

    #[builder(default)]
    version: Option<String>,

    hook: HookAction,

    #[builder(default)]
    pick_regex: Option<String>,

    /// a glob of executable file in zip. for help to comfirm exe bin
    #[builder(default)]
    exe_glob: Option<String>,

    source: Source,
}

#[derive(Debug, Getters, Setters, Clone, Builder, Serialize, Deserialize)]
#[getset(get = "pub", set, get)]
#[builder(pattern = "mutable", setter(into, strip_option))]
pub struct HookAction {
    install: Option<String>,
    update: Option<String>,
    extract: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Source {
    Github {
        owner: String,
        repo: String,
    }
}
