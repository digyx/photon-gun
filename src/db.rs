use sqlx::PgExecutor;
use tracing::debug;

use crate::healthcheck;

fn get_table_name(given: &str) -> String {
    format!("check_{}", base64::encode_config(given, base64::URL_SAFE))
}

pub async fn create_healthcheck_table<'a, E>(pool: E, service_name: &str) -> Result<(), sqlx::Error>
where
    E: PgExecutor<'a>,
{
    let sql_query = format!(
        "
        CREATE TABLE IF NOT EXISTS {} (
            id      SERIAL PRIMARY KEY,
            start_time   TIMESTAMP NOT NULL,
            elapsed_time INTERVAL  NOT NULL,
            pass    BOOL NOT NULL,
            message TEXT
        )
    ",
        get_table_name(service_name)
    );
    let result = sqlx::query(&sql_query).execute(pool).await?;

    debug!(rows_affected = result.rows_affected());
    Ok(())
}

pub async fn record_healthcheck<'a, E>(
    pool: E,
    check: healthcheck::HealthcheckResult,
) -> Result<(), sqlx::Error>
where
    E: PgExecutor<'a>,
{
    let sql_query = format!(
        "INSERT INTO {} (start_time, elapsed_time, pass, message) VALUES (To_Timestamp($1), $2::interval, $3, $4)",
        get_table_name(&check.service_name),
    );
    let result = sqlx::query(&sql_query)
        // To_Timestamp takes type "double precision", aka. Rust f64
        .bind(check.start_time)
        // Since elapsed_time is passed as a string, we specify the type
        .bind(check.elapsed_time)
        .bind(check.pass)
        .bind(check.message)
        .execute(pool)
        .await?;

    debug!(result.rows_affected = result.rows_affected());
    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("vorona","check_dm9yb25h")]
    fn success_get_table_name(#[case] name: &str, #[case] expected: &str) {
        let res = get_table_name(name);

        assert_eq!(res, expected);
    }
}
