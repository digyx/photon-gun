use sqlx::PgExecutor;
use tracing::{debug, error, warn};

use crate::healthcheck;

// Used to protect against SQL injection for the table names
// I almost feel embarrassed for having this function, but it works well so...
pub fn encode_table_name(given: &str) -> String {
    format!(
        // Since b64 includes "illegal" postgres characters, we need quotation marks to use them.
        // It's better to do it here rather than the actual queries since one could easily forget
        // to include them in the query
        "\"check_{}\"",
        base64::encode_config(given, base64::URL_SAFE)
    )
}

// Strip the check_ prefix and decode the base64
// This shouldn't be able to fail anymore...but I'm still skeptical
pub fn decode_table_name(given: &str) -> Option<String> {
    let b64 = match given.strip_prefix("check_") {
        Some(val) => val,
        None => {
            warn!(msg = "database table name with no 'check_' prefix", table_name = %given);
            given
        }
    };

    let bytes = match base64::decode_config(b64, base64::URL_SAFE) {
        Ok(val) => val,
        Err(err) => {
            error!(%err);
            return None;
        }
    };

    let utf8 = match std::str::from_utf8(&bytes[..]) {
        Ok(val) => val.to_owned(),
        Err(err) => {
            error!(%err);
            return None;
        }
    };

    Some(utf8)
}

// Each healthcheck gets its own postgres table since the "name" column would be absolutely
// redundant.  We also only query for, at most, one check at a time
pub async fn create_healthcheck_table<'a, E>(pool: E, service_name: &str) -> Result<(), sqlx::Error>
where
    E: PgExecutor<'a>,
{
    let sql_query = format!(
        "
        CREATE TABLE IF NOT EXISTS {} (
            id           SERIAL PRIMARY KEY,
            start_time   TIMESTAMP NOT NULL,
            elapsed_time INTERVAL  NOT NULL,
            pass         BOOL      NOT NULL,
            message      TEXT
        )
    ",
        encode_table_name(service_name)
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
        encode_table_name(&check.service_name),
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
    #[case("vorona", "\"check_dm9yb25h\"")]
    #[case("google", "\"check_Z29vZ2xl\"")]
    #[case("test", "\"check_dGVzdA==\"")]
    #[case("random", "\"check_cmFuZG9t\"")]
    fn success_encode_table_name(#[case] name: &str, #[case] expected: &str) {
        let res = encode_table_name(name);
        assert_eq!(res, expected);
    }

    #[rstest]
    #[case("check_dm9yb25h", "vorona")]
    #[case("check_Z29vZ2xl", "google")]
    #[case("check_dGVzdA==", "test")]
    #[case("check_cmFuZG9t", "random")]
    fn success_decode_table_name(#[case] name: &str, #[case] expected: &str) {
        let res = decode_table_name(name).unwrap();
        assert_eq!(res, expected);
    }
}
