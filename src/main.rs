use std::{
    fs::{create_dir_all, File},
    io::Read,
};

use rbatis::{crud::CRUD, crud_table, rbatis::Rbatis, DateTimeNative};

#[tokio::main]
async fn main() {
    // let rb = Rbatis::new();
    // // 连接数据库,自动判断驱动类型"mysql://*","postgres://*","sqlite://*","mssql://*"加载驱动
    // rb.link("sqlite://cats.db").await.unwrap();
    // 自定义连接池参数。(可选)
    // use crate::core::db::DBPoolOptions;
    // let mut opt =DBPoolOptions::new();
    // opt.max_connections=100;
    // rb.link_opt("mysql://root:123456@localhost:3306/test",&opt).await.unwrap();

    //启用日志输出，你也可以使用其他日志框架，这个不限定的
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .filter_module(env!("CARGO_CRATE_NAME"), log::LevelFilter::Trace)
        .init();
    let rb = init_sqlite().await;
    let activity = BizActivity {
        id: Some("12312".to_string()),
        name: Some("12312".to_string()),
        pc_link: None,
        h5_link: None,
        pc_banner_img: None,
        h5_banner_img: None,
        sort: Some("1".to_string()),
        status: Some(1),
        remark: None,
        create_time: Some(DateTimeNative::now()),
        version: Some(1),
        delete_flag: Some(1),
    };
    rb.remove_by_column::<BizActivity, _>("id", &activity.id)
        .await.unwrap();
    let r = rb.save(&activity, &[]).await;
    println!("{:?}", r);
}

#[crud_table]
#[derive(Clone, Debug)]
pub struct BizActivity {
    pub id: Option<String>,
    pub name: Option<String>,
    pub pc_link: Option<String>,
    pub h5_link: Option<String>,
    pub pc_banner_img: Option<String>,
    pub h5_banner_img: Option<String>,
    pub sort: Option<String>,
    pub status: Option<i32>,
    pub remark: Option<String>,
    pub create_time: Option<rbatis::DateTimeNative>,
    pub version: Option<i64>,
    pub delete_flag: Option<i32>,
}

/// make a sqlite-rbatis
pub async fn init_sqlite() -> Rbatis {
    if File::open("./target/sqlite.db").is_err() {
        create_dir_all("./target/");
        let f = File::create("./target/sqlite.db").unwrap();
        drop(f);
    }

    // init rbatis
    let rb = Rbatis::new();
    rb.link("sqlite://./target/sqlite.db").await.unwrap();

    // run sql create table
    let mut f = File::open("table_sqlite.sql").unwrap();
    let mut sql = String::new();
    f.read_to_string(&mut sql).unwrap();
    rb.exec(&sql, vec![]).await;

    // custom connection option
    // //mysql
    // // let db_cfg=DBConnectOption::from("mysql://root:123456@localhost:3306/test")?;
    // let db_cfg=DBConnectOption::from("sqlite://../target/sqlite.db")?;
    // rb.link_cfg(&db_cfg,PoolOptions::new());

    // custom pool
    // let mut opt = PoolOptions::new();
    // opt.max_size = 20;
    // rb.link_opt("sqlite://../target/sqlite.db", &opt).await.unwrap();
    return rb;
}
