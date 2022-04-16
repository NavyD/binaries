use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use getset::{Getters, Setters};

use crate::binary::github;

#[derive(Debug, Getters, Setters)]
#[getset(get = "pub")]
pub struct Config {
    archs: Option<HashMap<String, HashSet<String>>>,
    executable_dir: Option<PathBuf>,
    github_bins: Vec<github::BinaryConfig>,
}

struct GithubBinConfig {
    owner: String,
    repo: String,
    version: Option<String>,
    tag: Option<String>,
    pattern: Option<String>,
}

#[derive(Debug, Getters, Setters)]
#[getset(get = "pub")]
pub struct Hook {
    work_dir: Option<PathBuf>,
    action: HookAction,
}

#[derive(Debug, Getters, Setters)]
#[getset(get = "pub")]
pub struct HookAction {
    install: Option<String>,
    update: Option<String>,
    extract: Option<String>,
}
