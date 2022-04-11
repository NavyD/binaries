use chrono::{DateTime, Local};
use once_cell::sync::Lazy;

// static RB: Lazy<Rbatis> = Lazy::new(Rbatis::new);

pub mod update_info;

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use sea_orm::{
        ConnectionTrait, Database, DatabaseConnection, DbBackend, Schema, Statement,
        StatementBuilder,
    };
    use std::{
        fs::{create_dir_all, File},
        path::{Path, PathBuf},
        sync::Once,
    };
    use tokio::{fs::read_to_string, runtime::Runtime};

    use super::*;

    static INIT: Once = Once::new();
    pub static TOKIO_RT: Lazy<Runtime> = Lazy::new(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    });

    // pub static DB: Lazy<DatabaseConnection> = Lazy::new(|| {
    //     TOKIO_RT
    //         .block_on(async {
    //             let setup_sql = "table_sqlite.sql".parse::<PathBuf>().unwrap();
    //             let db_url = "sqlite::memory:";

    //             let db = Database::connect(db_url).await?;
    //             let sql = read_to_string(setup_sql).await?;
    //             let s = Statement {
    //                 db_backend: sea_orm::DatabaseBackend::Sqlite,
    //                 sql,
    //                 values: None,
    //             };
    //             let a = db.execute(s).await?;
    //             assert!(a.rows_affected() == 1);
    //             Ok::<_, Error>(db)
    //         })
    //         .unwrap()
    // });
}
