use chrono::{DateTime, Local};
use rbatis::{crud_table, sql};

use super::RB;

#[derive(Clone, Debug)]
#[crud_table]
pub struct UpdatedInfo {
    id: usize,
    name: String,
    version: String,
    updated_time: DateTime<Local>,
    create_time: DateTime<Local>,
}

#[sql(RB, "select * from updated_info")]
async fn select() -> Vec<UpdatedInfo> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_select() {
        let info = select().await.unwrap();
        println!("{:?}", info);
    }
}
