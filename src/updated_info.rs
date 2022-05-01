use anyhow::Result;
use chrono::{DateTime, Local};
use derive_builder::Builder;
use getset::{Getters, Setters};
use sqlx::SqlitePool;

// static RB: Lazy<Rbatis> = Lazy::new(Rbatis::new);

#[derive(sqlx::FromRow, Debug, Clone, PartialEq, Eq, Getters, Setters, Builder)]
#[getset(get = "pub", set = "pub")]
#[builder(setter(into))]
pub struct UpdatedInfo {
    #[builder(default = "0")]
    id: u32,
    name: String,
    version: String,
    url: String,
    source: String,
    #[builder(default = "Local::now()")]
    updated_time: DateTime<Local>,
    #[builder(default = "Local::now()")]
    create_time: DateTime<Local>,
}

#[derive(Debug, Clone)]
pub struct Mapper {
    pub pool: SqlitePool,
}

impl Mapper {
    pub async fn select_all(&self) -> Result<Vec<UpdatedInfo>> {
        sqlx::query_as::<_, UpdatedInfo>("select * from updated_info")
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn select_list_by_name(&self, name: &str) -> Result<Vec<UpdatedInfo>> {
        sqlx::query_as::<_, UpdatedInfo>("select * from updated_info where name = ?")
            .bind(name)
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn insert(&self, info: &UpdatedInfo) -> Result<u32> {
        sqlx::query(
            "insert into updated_info(name, version, source, url, updated_time, create_time) values(?, ?, ?, ?, ?, ?)",
        )
        .bind(&info.name)
        .bind(&info.version)
        .bind(&info.source())
        .bind(&info.url())
        .bind(&info.updated_time)
        .bind(&info.create_time)
        .execute(&self.pool)
        .await
        .map(|e| e.last_insert_rowid() as u32)
        .map_err(Into::into)
    }

    pub async fn delete_by_name(&self, name: &str) -> Result<usize> {
        sqlx::query("delete from updated_info where name = ?")
            .bind(name)
            .execute(&self.pool)
            .await
            .map(|r| r.rows_affected() as usize)
            .map_err(Into::into)
    }
}
#[cfg(test)]
mod tests {
    use anyhow::Error;
    use chrono::{TimeZone, Utc};
    use futures_util::TryStreamExt;
    use log::trace;
    use once_cell::sync::Lazy;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::thread;
    use tokio::{fs::read_to_string, runtime::Runtime};

    use crate::config::Source;

    use super::*;

    pub static TOKIO_RT: Lazy<Runtime> = Lazy::new(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    });

    static MAPPER: Lazy<Mapper> = Lazy::new(|| {
        thread::spawn(|| {
            let pool = TOKIO_RT
                .block_on(async {
                    let pool = SqlitePoolOptions::new()
                        .max_connections(4)
                        .connect("sqlite::memory:")
                        .await?;
                    let sql = read_to_string("schema.sql").await?
                        + "\n"
                        + &read_to_string("data.sql").await?;
                    trace!("setup sql: {}", sql);
                    let mut rows = sqlx::query(&sql).execute_many(&pool).await;
                    while let Some(row) = rows.try_next().await? {
                        trace!("get row: {:?}", row);
                    }
                    Ok::<_, Error>(pool)
                })
                .unwrap();
            Mapper { pool }
        })
        .join()
        .unwrap()
    });

    #[test]
    fn test_select_name() -> Result<()> {
        TOKIO_RT.block_on(async {
            let name = "btm";
            let infos = MAPPER.select_list_by_name(name).await?;
            assert_eq!(infos.len(), 2);
            infos.iter().for_each(|i| {
                assert_eq!(i.name(), name);
            });

            let res = MAPPER.select_list_by_name("___no___").await?;
            assert!(res.is_empty());
            Ok::<_, Error>(())
        })
    }

    #[test]
    fn test_select_all() -> Result<()> {
        let parse_date = |s: &str| -> Result<DateTime<Utc>> {
            // let d = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")?;
            // let a = Local.from_local_datetime(&d).unwrap();
            let a = Utc.datetime_from_str(s, "%Y-%m-%d %H:%M:%S")?;
            Ok(a)
        };

        TOKIO_RT.block_on(async {
            let infos = MAPPER.select_all().await?;
            assert_eq!(infos.len(), 3);

            assert_eq!(infos[0].id, 1);
            assert_eq!(infos[0].name, "btm");
            assert_eq!(infos[0].version, "v0.6.0");
            assert_eq!(infos[0].source, r#"{"github":{"owner":"ClementTsang","repo":"bottom"}}"#);
            assert_eq!(infos[0].url, r#"https://github.com/ClementTsang/bottom/releases/download/0.6.8/bottom_x86_64-unknown-linux-gnu.tar.gz"#);
            assert_eq!(infos[0].create_time, parse_date("2020-06-17 20:10:23")?);
            assert_eq!(infos[0].updated_time, parse_date("2020-06-17 20:10:23")?);

            assert_eq!(infos[1].id, 2);
            assert_eq!(infos[1].name, "tldr");
            assert_eq!(infos[1].version, "v0.2.0");
            assert_eq!(infos[0].create_time, parse_date("2020-06-17 20:10:23")?);
            // assert_eq!(infos[0].updated_time, parse_date("2020-07-17 21:10:23")?);

            assert_eq!(infos[2].id, 3);
            assert_eq!(infos[2].name, "btm");
            assert_eq!(infos[2].version, "v0.7.0");
            assert_eq!(infos[2].create_time, parse_date("2021-06-17 20:10:23")?);
            assert_eq!(infos[2].updated_time, parse_date("2021-06-17 20:10:23")?);
            Ok::<_, Error>(())
        })
    }

    #[test]
    fn test_insert() -> Result<()> {
        TOKIO_RT.block_on(async {
            let source = serde_json::to_string(&"github:a/b".parse::<Source>()?)?;
            let info = UpdatedInfoBuilder::default()
                .name("tldr")
                .version("v0.3.0")
                .source(&source)
                .url("https://github.com/dbrgn/tealdeer/releases/download/v1.5.0/tealdeer-linux-x86_64-musl")
                .build()?;

            let last_id = MAPPER.insert(&info).await?;
            assert_ne!(info.id, last_id);
            assert_eq!(last_id, 4);

            let mut res = info.clone();
            res.id = info.id;
            assert_eq!(info, res);
            Ok::<_, Error>(())
        })
    }

    #[test]
    fn feature() -> Result<()> {
        TOKIO_RT.block_on(async {
            assert_eq!(MAPPER.delete_by_name("btm").await?, 2);
            assert_eq!(MAPPER.delete_by_name("tldr").await?, 1);
            assert_eq!(MAPPER.delete_by_name("btm").await?, 0);
            assert_eq!(MAPPER.delete_by_name("__no_name_#_").await?, 0);
            Ok::<_, Error>(())
        })
    }
}
