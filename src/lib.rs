#![allow(unused)]

pub mod binary;
pub mod config;
pub mod manager;
pub mod updated_info;
pub mod util;

pub static CRATE_NAME: &str = env!("CARGO_CRATE_NAME");

#[cfg(test)]
mod tests {
    use super::*;
    use log::LevelFilter;
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
        });
    }
}
