use chrono::{DateTime, Local};
use once_cell::sync::Lazy;
use rbatis::rbatis::Rbatis;

static RB: Lazy<Rbatis> = Lazy::new(Rbatis::new);

pub mod update_info;

#[cfg(test)]
mod tests {
    use std::{
        fs::{create_dir_all, File},
        path::PathBuf,
        sync::Once,
    };

    use tokio::{fs::read_to_string, runtime::Runtime};

    use super::*;

    static INIT: Once = Once::new();
    static TOKIO_RT: Lazy<Runtime> = Lazy::new(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    });

    #[ctor::ctor]
    fn init() {
        INIT.call_once(|| {
            let path = "./target/sqlite.db".parse::<PathBuf>().unwrap();
            if File::open(&path).is_err() {
                create_dir_all(path.parent().unwrap()).unwrap();
                File::create(&path).unwrap();
            }
            let setup_sql = "table_sqlite.sql";
            TOKIO_RT.block_on(async {
                RB.link(&format!("sqlite://{}", path.display()))
                    .await
                    .unwrap();
                let s = read_to_string(setup_sql).await.unwrap();
                log::trace!("sql: {}", s);
                println!("sql: {}", s);
                RB.exec(&s, vec![]).await.unwrap();
            });
        });
    }
}
