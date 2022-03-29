use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::postgres;
use tracing::debug;

pub async fn create_healthcheck_table(
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

pub async fn record_healthcheck(
    pool: &postgres::PgPool,
    table_name: &str,
    pass: bool,
    message: &str,
) -> Result<(), sqlx::Error> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let sql_query = format!(
        "INSERT INTO {} (time, pass, message) VALUES ($1, $2, $3)",
        table_name
    );
    let result = sqlx::query(&sql_query)
        .bind(now)
        .bind(pass)
        .bind(message)
        .execute(pool)
        .await?;

    debug!(result.rows_affects = result.rows_affected());
    Ok(())
}
