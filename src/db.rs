use std::sync::Arc;

use sqlx::{FromRow, PgPool};

use crate::{healthcheck::Healthcheck, HealthcheckResult};

#[derive(Debug, FromRow)]
struct HealthcheckSchema {
    id: i32,
    #[allow(dead_code)]
    name: Option<String>,
    endpoint: String,
    interval: i32,
}

pub async fn initialize_tables(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "
        CREATE TABLE IF NOT  EXISTS healthchecks (
            id       SERIAL  PRIMARY KEY,
            name     TEXT,
            endpoint TEXT    NOT NULL,
            interval INTEGER NOT NULL
        )
        ",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS healthcheck_results (
            id           BIGSERIAL PRIMARY KEY,
            check_id     INTEGER   REFERENCES healthchecks,
            start_time   TIMESTAMP NOT NULL,
            elapsed_time INTERVAL  NOT NULL,
            pass         BOOL      NOT NULL,
            message      TEXT
        )
        ",
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_healthchecks(pool: Arc<PgPool>) -> Result<Vec<Healthcheck>, sqlx::Error> {
    let sql_query = "SELECT * FROM healthchecks";

    let res: Vec<HealthcheckSchema> = sqlx::query_as(sql_query).fetch_all(&*pool.clone()).await?;

    if res.is_empty() {
        return Ok(vec![]);
    }

    let healthchecks = res
        .into_iter()
        .map(|x| Healthcheck::new(x.id, x.endpoint, x.interval, pool.clone()))
        .collect();

    Ok(healthchecks)
}

pub(crate) async fn insert_healthcheck_result(
    pool: &PgPool,
    result: HealthcheckResult,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "
        INSERT INTO healthcheck_results (check_id, start_time, elapsed_time, pass, message)
        VALUES ($1, $2, $3, $4, $5)
        ",
    )
    .bind(result.check_id)
    .bind(result.start_time)
    .bind(result.elapsed_time)
    .bind(result.pass)
    .bind(result.message)
    .execute(pool)
    .await?;

    Ok(())
}
