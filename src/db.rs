use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::postgres;
use tracing::debug;

pub async fn create_basic_check_table(
    pool: &postgres::PgPool,
    table_name: &str,
) -> Result<(), sqlx::Error> {
    let sql_query = format!(
        "
        CREATE TABLE IF NOT EXISTS {} (
            id      SERIAL PRIMARY KEY,
            time    BIGINT NOT NULL UNIQUE,
            pass    BOOL   NOT NULL,
            message TEXT
        )
    ",
        table_name
    );
    let result = sqlx::query(&sql_query).execute(pool).await?;

    debug!(rows_affected = result.rows_affected());
    Ok(())
}

pub async fn record_basic_check(
    pool: &postgres::PgPool,
    table_name: &str,
    pass: bool,
) -> Result<(), sqlx::Error> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let sql_query = format!("INSERT INTO {} (time, pass) VALUES ($1, $2)", table_name);
    let result = sqlx::query(&sql_query)
        .bind(now)
        .bind(pass)
        .execute(pool)
        .await?;

    debug!(result.rows_affects = result.rows_affected());
    Ok(())
}
