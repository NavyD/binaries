use anyhow::Error;
use chrono::{DateTime, Local};
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "updated_info")]
pub struct Model {
    #[sea_orm(primary_key)]
    id: u32,
    name: String,
    version: String,
    updated_time: DateTime<Local>,
    create_time: DateTime<Local>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf};

    use anyhow::Result;
    use sea_orm::{QuerySelect, Database, Statement, ConnectionTrait};
    use tokio::fs::read_to_string;

    use super::*;
    use crate::dao::tests::{ TOKIO_RT};

    #[test]
    fn test_select() {
        TOKIO_RT.block_on(async {
            let setup_sql = "table_sqlite.sql".parse::<PathBuf>().unwrap();
            let db_url = "sqlite::memory:";

            let db = Database::connect(db_url).await?;
            let sql = read_to_string(setup_sql).await?;
            let s = Statement {
                db_backend: sea_orm::DatabaseBackend::Sqlite,
                sql,
                values: None,
            };
            let a = db.execute(s).await?;
            let DB = db;

            let a = <crate::dao::update_info::ActiveModel as sea_orm::ActiveModelTrait>::default();
            let a = Entity::find().all(&DB).await?;
            assert_eq!(a.len(), 2);
            assert_eq!(a[0].name, "btm");
            println!("{:?}", a);
            Ok::<_, Error>(())
        }).unwrap()
    }
}
