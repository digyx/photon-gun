use sqlx::postgres;
use tracing::debug;

use crate::healthcheck;

fn get_table_name(given: &str) -> String {
    format!("check_{}", base64::encode_config(given, base64::URL_SAFE))
}

pub async fn create_healthcheck_table(
    pool: &postgres::PgPool,
    service_name: &str,
) -> Result<(), sqlx::Error> {
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

pub async fn record_healthcheck(
    pool: &postgres::PgPool,
    check: healthcheck::HealthcheckResult,
) -> Result<(), sqlx::Error> {
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
    #[test]
    fn get_table_name() {
        let expected = "check_dm9yb25h";
        let res = super::get_table_name("vorona");

        assert_eq!(res, expected);
    }
}
