use hyper::{Body, Request, Response, StatusCode};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use sqlx::types::chrono;
use sqlx::{FromRow, PgExecutor};
use tracing::{debug, error};

use crate::db;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct UriQueries {
    #[serde(alias = "service")]
    service_name: String,
    resolution: Option<Resolution>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Resolution {
    Second,
    Minute,
    Hour,
    Day,
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let res = match self {
            Resolution::Second => "second",
            Resolution::Minute => "minute",
            Resolution::Hour => "hour",
            Resolution::Day => "day",
        };

        write!(f, "{}", res)
    }
}

#[derive(Debug, FromRow)]
struct HealthcheckSummary {
    time_window: chrono::NaiveDateTime,
    pass: i64,
    fail: i64,
}

impl Serialize for HealthcheckSummary {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_struct("HealthcheckSummary", 3)?;
        s.serialize_field("time_window", &self.time_window.to_string())?;
        s.serialize_field("pass", &self.pass)?;
        s.serialize_field("fail", &self.fail)?;
        s.end()
    }
}

pub async fn handle<'a, E>(req: Request<Body>, db_client: E) -> Response<Body>
where
    E: PgExecutor<'a>,
{
    let queries: UriQueries = match super::decode_url_params(req.uri()) {
        Ok(queries) => queries,
        Err(err) => {
            debug!(%err, msg = "Invalid URL paramters given.");
            return super::response_with_string_body(StatusCode::BAD_REQUEST, err);
        }
    };

    let sql_query = format!(
        "
        SELECT
            date_trunc('{}', start_time) as time_window,
            count(*) filter(where \"pass\") as pass,
            count(*) filter(where not \"pass\") as fail
        FROM {}
        GROUP BY time_window
        ORDER BY time_window DESC
        LIMIT 60
    ",
        queries.resolution.unwrap_or(Resolution::Minute),
        db::get_table_name(&queries.service_name),
    );

    let result: Vec<HealthcheckSummary> =
        match sqlx::query_as(&sql_query).fetch_all(db_client).await {
            Ok(result) => result,
            Err(err) => {
                error!(%err);
                return super::response_with_string_body(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to decode postgres row",
                );
            }
        };

    let body = match serde_json::to_string(&result) {
        Ok(value) => value,
        Err(err) => {
            error!(%err);
            err.to_string()
        }
    };

    Response::new(Body::from(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    use hyper::Uri;
    use rstest::rstest;

    impl UriQueries {
        fn new(service_name: String, resolution: Option<Resolution>) -> UriQueries {
            UriQueries {
                service_name,
                resolution,
            }
        }
    }

    #[test]
    fn success_healthcheck_summary_serialization() {
        let input = HealthcheckSummary {
            time_window: chrono::NaiveDateTime::from_timestamp(1648771200, 0),
            pass: 60,
            fail: 6,
        };
        let expected = "{\"time_window\":\"2022-04-01 00:00:00\",\"pass\":60,\"fail\":6}";

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
        Uri::try_from("/test?service=test&resolution=second"),
        UriQueries::new("test".into(), Some(Resolution::Second))
    )]
    #[case(
        Uri::try_from("/test?service=test&resolution=minute"),
        UriQueries::new("test".into(), Some(Resolution::Minute))
    )]
    #[case(
        Uri::try_from("/test?service=test&resolution=hour"),
        UriQueries::new("test".into(), Some(Resolution::Hour))
    )]
    #[case(
        Uri::try_from("/test?service=test&resolution=day"),
        UriQueries::new("test".into(), Some(Resolution::Day))
    )]
    fn success_decode_url_params(
        #[case] input: Result<Uri, http::uri::InvalidUri>,
        #[case] expected: UriQueries,
    ) {
        let res: UriQueries = super::super::decode_url_params(&input.unwrap()).unwrap();
        assert_eq!(res, expected);
    }
}
