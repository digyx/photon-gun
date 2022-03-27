use std::time::{UNIX_EPOCH, SystemTime};

use sqlx::postgres;
use tracing::debug;

pub async fn record_basic_check(pool: &postgres::PgPool, table_name: &str, pass: bool) -> Result<(), sqlx::Error> {
    let now = SystemTime::now().
        duration_since(UNIX_EPOCH).
        unwrap().
        as_secs() as i64;

    let sql_query = format!("INSERT INTO {} (time, pass) VALUES ($1, $2)", table_name);
    let result = sqlx::query(&sql_query)
        .bind(now)
        .bind(pass)
        .execute(pool)
        .await?;

    debug!(result.rows_affects = result.rows_affected());
    Ok(())
}

