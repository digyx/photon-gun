use sqlx::postgres::types::PgInterval;
use sqlx::types::chrono;
use sqlx::{FromRow, PgPool};

use crate::{Healthcheck, HealthcheckList, HealthcheckResult, HealthcheckResultList};

#[derive(Debug, FromRow)]
struct HealthcheckSchema {
    id: i32,
    name: Option<String>,
    endpoint: String,
    interval: i32,
    enabled: bool,
}

impl From<Healthcheck> for HealthcheckSchema {
    fn from(check: Healthcheck) -> Self {
        HealthcheckSchema {
            id: check.id,
            name: check.name,
            endpoint: check.endpoint,
            interval: check.interval,
            enabled: check.enabled,
        }
    }
}

impl From<HealthcheckSchema> for Healthcheck {
    fn from(schema: HealthcheckSchema) -> Self {
        Healthcheck {
            id: schema.id,
            name: schema.name,
            endpoint: schema.endpoint,
            interval: schema.interval,
            enabled: schema.enabled,
        }
    }
}

#[derive(Debug, FromRow)]
struct HealthcheckResultSchema {
    id: i64,
    check_id: i32,
    start_time: chrono::NaiveDateTime,
    elapsed_time: PgInterval,
    pass: bool,
    message: Option<String>,
}

impl From<HealthcheckResult> for HealthcheckResultSchema {
    fn from(res: HealthcheckResult) -> Self {
        HealthcheckResultSchema {
            id: res.id,
            check_id: res.check_id,
            start_time: chrono::NaiveDateTime::from_timestamp(res.start_time, 0),
            elapsed_time: PgInterval {
                months: 0,
                days: 0,
                microseconds: res.elapsed_time,
            },
            pass: res.pass,
            message: res.message,
        }
    }
}

impl From<HealthcheckResultSchema> for HealthcheckResult {
    fn from(res: HealthcheckResultSchema) -> Self {
        HealthcheckResult {
            id: res.id,
            check_id: res.check_id,
            start_time: res.start_time.timestamp(),
            elapsed_time: res.elapsed_time.microseconds,
            pass: res.pass,
            message: res.message,
        }
    }
}

pub async fn initialize_tables(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "
        CREATE TABLE IF NOT  EXISTS healthchecks (
            id       SERIAL  PRIMARY KEY,
            name     TEXT,
            endpoint TEXT    NOT NULL,
            interval INTEGER NOT NULL,
            enabled  BOOL    NOT NULL
        )
        ",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS healthcheck_results (
            id           BIGSERIAL PRIMARY KEY,
            check_id     INTEGER   REFERENCES healthchecks ON DELETE CASCADE,
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

// ==================== Healthcheck Operations ====================
pub(crate) async fn get_healthcheck(pool: &PgPool, id: i32) -> Result<Healthcheck, sqlx::Error> {
    let sql_query = "SELECT * FROM healthchecks WHERE id=$1";
    let res: HealthcheckSchema = sqlx::query_as(sql_query).bind(id).fetch_one(pool).await?;
    Ok(res.into())
}

pub(crate) async fn list_healthchecks(
    pool: &PgPool,
    enabled: bool,
    limit: i32,
) -> Result<HealthcheckList, sqlx::Error> {
    let sql_query = "SELECT * FROM healthchecks WHERE enabled=$1 LIMIT $2";
    let res: Vec<HealthcheckSchema> = sqlx::query_as(sql_query)
        .bind(enabled)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(HealthcheckList {
        // Translate HealthcheckSchema structs to Healthcheck
        healthchecks: res.into_iter().map(|x| x.into()).collect(),
    })
}

pub(crate) async fn insert_healthcheck(
    pool: &PgPool,
    check: &Healthcheck,
) -> Result<i32, sqlx::Error> {
    #[derive(FromRow)]
    struct ID {
        id: i32,
    }

    let sql_query =
        "INSERT INTO healthchecks (name, endpoint, interval) VALUES ($1, $2, $3) RETURNING id";
    let res: ID = sqlx::query_as(sql_query)
        .bind(&check.name)
        .bind(&check.endpoint)
        .bind(check.interval)
        .fetch_one(pool)
        .await?;
    Ok(res.id)
}

pub(crate) async fn update_healthcheck_name(
    pool: &PgPool,
    id: i32,
    name: &str,
) -> Result<(), sqlx::Error> {
    let sql_query = "UPDATE healthchecks SET name=$1 WHERE id=$2";
    sqlx::query(sql_query)
        .bind(name)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn update_healthcheck_endpoint(
    pool: &PgPool,
    id: i32,
    endpoint: &str,
) -> Result<(), sqlx::Error> {
    let sql_query = "UPDATE healthchecks SET endpoint=$1 WHERE id=$2";
    sqlx::query(sql_query)
        .bind(endpoint)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn update_healthcheck_interval(
    pool: &PgPool,
    id: i32,
    interval: i32,
) -> Result<(), sqlx::Error> {
    let sql_query = "UPDATE healthchecks SET interval=$1 WHERE id=$2";
    sqlx::query(sql_query)
        .bind(interval)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn delete_healthcheck(pool: &PgPool, id: i32) -> Result<Healthcheck, sqlx::Error> {
    let sql_query = "DELETE FROM healthchecks WHERE id=$1 RETURNING healthchecks.*";
    let res: HealthcheckSchema = sqlx::query_as(sql_query).bind(id).fetch_one(pool).await?;

    Ok(res.into())
}

pub(crate) async fn enable_healthcheck(pool: &PgPool, id: i32) -> Result<(), sqlx::Error> {
    let sql_query = "UPDATE healthchecks SET enabled=true WHERE id=$1";
    sqlx::query(sql_query).bind(id).execute(pool).await?;

    Ok(())
}

pub(crate) async fn disable_healthcheck(pool: &PgPool, id: i32) -> Result<(), sqlx::Error> {
    let sql_query = "UPDATE healthchecks SET enabled=false WHERE id=$1";
    sqlx::query(sql_query).bind(id).execute(pool).await?;

    Ok(())
}

// ==================== Healthcheck Result Operations ====================
pub(crate) async fn list_healthcheck_results(
    pool: &PgPool,
    id: i32,
    limit: i32,
) -> Result<HealthcheckResultList, sqlx::Error> {
    // check_id can be filtered out
    let sql_query = "
        SELECT healthcheck_results.*
        FROM healthcheck_results
        INNER JOIN healthchecks ON healthcheck_results.check_id=healthchecks.id
        WHERE check_id=$1
        ORDER BY healthcheck_results.id DESC
        LIMIT $2
        ";

    let result: Vec<HealthcheckResultSchema> = sqlx::query_as(sql_query)
        .bind(id)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    let res = result.into_iter().map(|x| x.into()).collect();
    Ok(HealthcheckResultList {
        healthcheck_results: res,
    })
}

pub(crate) async fn insert_healthcheck_result(
    pool: &PgPool,
    result: HealthcheckResult,
) -> Result<(), sqlx::Error> {
    let params: HealthcheckResultSchema = result.into();

    sqlx::query(
        "
        INSERT INTO healthcheck_results (check_id, start_time, elapsed_time, pass, message)
        VALUES ($1, $2, $3, $4, $5)
        ",
    )
    .bind(params.check_id)
    .bind(params.start_time)
    .bind(params.elapsed_time)
    .bind(params.pass)
    .bind(params.message)
    .execute(pool)
    .await?;

    Ok(())
}
