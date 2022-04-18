#![allow(unused)]

pub mod source;
pub mod config;
pub mod manager;
pub mod updated_info;
pub mod util;

pub static CRATE_NAME: &str = env!("CARGO_CRATE_NAME");

#[cfg(test)]
mod tests {
    use super::*;
    use log::{info, warn, LevelFilter};
    use std::sync::Once;

    static INIT: Once = Once::new();

    #[ctor::ctor]
    fn init() {
        INIT.call_once(|| {
            env_logger::builder()
                .is_test(true)
                .filter_level(LevelFilter::Info)
                .filter_module(CRATE_NAME, LevelFilter::Trace)
                .init();
            match dotenv::dotenv() {
                Ok(p) => info!("loaded .env from {}", p.display()),
                Err(e) => warn!("not found .env"),
            }
        });
    }
}
