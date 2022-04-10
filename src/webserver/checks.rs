use hyper::{Body, Request, Response, StatusCode};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use sqlx::postgres::types::PgInterval;
use sqlx::types::chrono;
use sqlx::{FromRow, PgExecutor};
use tracing::error;

use crate::db;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct UriQueries {
    #[serde(alias = "service")]
    service_name: String,
    limit: Option<i32>,
}

#[allow(dead_code)]
#[derive(Debug, FromRow)]
struct Healthcheck {
    start_time: chrono::NaiveDateTime,
    elapsed_time: PgInterval,
    pass: bool,
    message: String,
}

#[derive(Debug, FromRow)]
struct TableNames {
    table_name: String,
}

impl Serialize for Healthcheck {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_struct("Healthcheck", 4)?;
        s.serialize_field("start_time", &self.start_time.to_string())?;
        // These are already stored as milliseconds, but PgInterval stores microseconds
        // Well...for values this small it does (microseconds, days, months)
        s.serialize_field("elapsed_time", &(self.elapsed_time.microseconds / 1000))?;
        s.serialize_field("pass", &self.pass)?;
        s.serialize_field("message", &self.message)?;
        s.end()
    }
}

pub async fn handle<'a, E>(req: Request<Body>, db_client: E) -> Response<Body>
where
    E: PgExecutor<'a>,
{
    // If there are no URI Queries, then this endpoint should return a list of healthcheck names
    // based on the (decoded) table names in the database
    if req.uri().query().is_none() {
        let check_names = match list_check_names(db_client).await {
            Ok(val) => val,
            Err(err) => {
                error!(%err);
                return super::response_with_string_body(StatusCode::INTERNAL_SERVER_ERROR, err);
            }
        };

        return Response::new(Body::from(check_names));
    }

    let queries: UriQueries = match super::decode_url_params(req.uri()) {
        Ok(val) => val,
        Err(_) => {
            return super::response_with_string_body(
                StatusCode::BAD_REQUEST,
                "invalid parameters: alphanumeric parameter 'service' is required",
            )
        }
    };

    let sql_query = format!(
        "
        SELECT
            start_time,
            elapsed_time,
            pass,
            message
        FROM {}
        ORDER BY id DESC
        LIMIT $1
        ",
        db::encode_table_name(&queries.service_name)
    );

    let result: Vec<Healthcheck> = match sqlx::query_as(&sql_query)
        .bind(queries.limit.unwrap_or(100))
        .fetch_all(db_client)
        .await
    {
        Ok(val) => val,
        Err(err) => {
            error!(%err);
            return super::response_with_string_body(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to decode postgres row",
            );
        }
    };

    let body = match serde_json::to_string(&result) {
        Ok(val) => val,
        Err(err) => {
            error!(%err);
            "failed to encode database row to JSON".into()
        }
    };

    Response::new(Body::from(body))
}

async fn list_check_names<'a, E>(db_client: E) -> Result<String, &'static str>
where
    E: PgExecutor<'a>,
{
    let rows = sqlx::query_as(
        "
            SELECT
                table_name
            FROM information_schema.tables
            WHERE table_schema='public'
            AND table_type='BASE TABLE'
            AND starts_with(table_name, 'check_')
            ",
    )
    .fetch_all(db_client)
    .await;

    // 'rows' variable exists for reabability
    let result: Vec<TableNames> = match rows {
        Ok(val) => val,
        Err(err) => {
            error!(%err);
            vec![]
        }
    };

    // The database returns a list of encoded table names, so we need to decode each name and then
    // collect the results in a Vector we can serialize
    let table_names: Vec<String> = result
        .iter()
        .map(|a| {
            match db::decode_table_name(&a.table_name) {
                Some(val) => val,
                // None is only returned when the decode fails
                // This *should* never happen since we're the ones who encode it and encode using
                // padding (no padding causeed a huge headache)
                None => {
                    error!(err = "failed to decode healthcheck name", table_name = %a.table_name);
                    String::new() // This is not a *good* solution, but it's all I can think of right now
                }
            }
        })
        .collect();

    let body = match serde_json::to_string(&table_names) {
        Ok(val) => val,
        Err(err) => {
            error!(%err);
            return Err("failed to encode healthcheck names to JSON");
        }
    };

    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    use hyper::Uri;
    use rstest::rstest;

    impl UriQueries {
        fn new(service_name: String, limit: Option<i32>) -> UriQueries {
            UriQueries {
                service_name,
                limit,
            }
        }
    }

    #[test]
    fn success_healthcheck_serialization() {
        let input = Healthcheck {
            start_time: chrono::NaiveDateTime::from_timestamp(1648771200, 0),
            elapsed_time: PgInterval {
                months: 0,
                days: 0,
                microseconds: 42_000,
            },
            pass: true,
            message: "".into(),
        };
        let expected = "{\"start_time\":\"2022-04-01 00:00:00\",\"elapsed_time\":42,\"pass\":true,\"message\":\"\"}";

        let res = serde_json::to_string(&input).unwrap();
        assert_eq!(&res, expected);
    }

    #[rstest]
    #[case(
        Uri::try_from("/test?service=test"),
        UriQueries::new("test".into(), None)
    )]
    #[case(
        Uri::try_from("/test?service=vorona"),
        UriQueries::new("vorona".into(), None)
    )]
    #[case(
        Uri::try_from("/test?service=test&limit=10"),
        UriQueries::new("test".into(), Some(10))
    )]
    fn success_decode_url_params(
        #[case] input: Result<Uri, http::uri::InvalidUri>,
        #[case] expected: UriQueries,
    ) {
        let res: UriQueries = super::super::decode_url_params(&input.unwrap()).unwrap();
        assert_eq!(res, expected);
    }
}
