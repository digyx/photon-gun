use sqlx::PgPool;

use crate::HealthcheckResult;

pub async fn initialize_tables(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "
        CREATE TABLE IF NOT  EXISTS basic_checks (
            id       SERIAL  PRIMARY KEY,
            name     TEXT,
            endpoint TEXT    NOT NULL,
            interval INTEGER NOT NULL
        )
        "
    ).execute(pool).await?;

    sqlx::query(
        "
        CREATE TABLE IF NOT  EXISTS lua_checks (
            id       SERIAL  PRIMARY KEY,
            name     TEXT    NOT NULL,
            script   TEXT    NOT NULL,
            interval INTEGER NOT NULL
        )
        "
    ).execute(pool).await?;

    sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS check_results (
            id           BIGSERIAL PRIMARY KEY,
            start_time   TIMESTAMP NOT NULL,
            elapsed_time INTERVAL  NOT NULL,
            pass         BOOL      NOT NULL,
            message      TEXT
        )
        "
    ).execute(pool).await?;

    Ok(())
}

pub(crate) async fn insert_basic_check_result(pool: &PgPool, result: HealthcheckResult) -> Result<(), sqlx::Error> {
    sqlx::query(
        "
        INSERT INTO basic_check_results (start_time, elapsed_time, pass, message)
        VALUES ($1, $2, $3, $4, $5)
        "
    )
        .bind(result.start_time)
        .bind(result.elapsed_time)
        .bind(result.pass)
        .bind(result.message)
        .execute(pool)
        .await?;

    Ok(())
}
