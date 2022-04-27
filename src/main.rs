use std::sync::Arc;

use anyhow::{Error, Result};
use directories::{BaseDirs, ProjectDirs};
use handlebars::Handlebars;
use reqwest::Client;

#[tokio::main]
async fn main() {
    //启用日志输出，你也可以使用其他日志框架，这个不限定的
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .filter_module(env!("CARGO_CRATE_NAME"), log::LevelFilter::Trace)
        .init();
}
