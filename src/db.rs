use sqlx::postgres::types::PgInterval;
use sqlx::types::chrono::NaiveDateTime;
use sqlx::types::uuid::Uuid;
use sqlx::{FromRow, PgPool};

use crate::Healthcheck;

#[derive(Debug, FromRow)]
struct HealthcheckSchema {
    _id: i32,
    agent_uuid: Uuid,
    pass: bool,
    endpoint: String,
    start_time: NaiveDateTime,
    elapsed_time: PgInterval,
}

impl From<HealthcheckSchema> for Healthcheck {
    fn from(schema: HealthcheckSchema) -> Self {
        Healthcheck {
            agent_uuid: schema.agent_uuid.to_string(),
            endpoint: schema.endpoint,
            pass: schema.pass,
            start_time: schema.start_time.timestamp(),
            elapsed_time: schema.elapsed_time.microseconds,
        }
    }
}

pub async fn initialize_tables(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "
        CREATE TABLE IF NOT  EXISTS healthchecks (
            id           SERIAL    PRIMARY KEY,
            agent_uuid   UUID      NOT NULL,
            pass         BOOL      NOT NULL,
            endpoint     TEXT      NOT NULL,
            start_time   TIMESTAMP NOT NULL,
            elapsed_time INTERVAL  NOT NULL
        )
        ",
    )
    .execute(pool)
    .await?;

    Ok(())
}

// ==================== Healthcheck Operations ====================
#[allow(dead_code)]
pub(crate) async fn list_healthchecks(
    pool: &PgPool,
    enabled: bool,
    limit: i32,
) -> Result<Vec<Healthcheck>, sqlx::Error> {
    let sql_query = "SELECT * FROM healthchecks WHERE enabled=$1 LIMIT $2";
    let res: Vec<HealthcheckSchema> = sqlx::query_as(sql_query)
        .bind(enabled)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(res.into_iter().map(|x| x.into()).collect())
}

pub(crate) async fn insert_healthcheck(
    pool: &PgPool,
    check: &Healthcheck,
) -> Result<(), sqlx::Error> {
    let agent_uuid = Uuid::parse_str(&check.agent_uuid).unwrap();
    let sql_query = "
        INSERT INTO healthchecks
            (agent_uuid, pass, endpoint, start_time, elapsed_time)
        VALUES
            ($1, $2, $3, to_timestamp($4), $5 * interval '1 microsecond')
        ";

    sqlx::query(sql_query)
        .bind(&agent_uuid)
        .bind(check.pass)
        .bind(&check.endpoint)
        .bind(check.start_time)
        .bind(check.elapsed_time)
        .execute(pool)
        .await?;
    Ok(())
}
