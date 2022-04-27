use hyper::{Body, Request, Response, StatusCode};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use sqlx::types::chrono;
use sqlx::{FromRow, PgPool};
use tracing::{debug, error};

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct UriQueries {
    #[serde(alias = "service")]
    id: Option<i32>,
    name: Option<String>,
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

// Display is used when constructing the SQL query
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

// Used for decoding the SQL result into
#[derive(Debug, FromRow)]
struct HealthcheckSummary {
    time_window: chrono::NaiveDateTime,
    pass: i64,
    fail: i64,
}

// And then to serialize into a JSON response
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

pub async fn handle(req: Request<Body>, db_client: &PgPool) -> Response<Body> {
    let queries: UriQueries = match super::decode_url_params(req.uri()) {
        Ok(queries) => queries,
        Err(err) => {
            debug!(%err);
            return super::response_with_string_body(StatusCode::BAD_REQUEST, err);
        }
    };

    let sql_query = format!(
        "
        SELECT
            date_trunc('{}', start_time) as time_window,
            count(*) filter(where \"pass\") as pass,
            count(*) filter(where not \"pass\") as fail
        FROM healthcheck_results
        INNER JOIN healthchecks ON healthcheck_results.check_id=healthchecks.id
        WHERE check_id=$1 OR name=$2
        GROUP BY time_window
        ORDER BY time_window DESC
        LIMIT 60
    ",
        queries.resolution.unwrap_or(Resolution::Minute)
    );

    let result: Vec<HealthcheckSummary> =
        // Run the actual query
        match sqlx::query_as(&sql_query)
            .bind(queries.id)
            .bind(queries.name)
            .fetch_all(db_client)
            .await {
            Ok(result) => result,
            // This error should never happen
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
        // Another error that should never happen
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
        fn new(
            id: Option<i32>,
            name: Option<String>,
            resolution: Option<Resolution>,
        ) -> UriQueries {
            UriQueries {
                id,
                name,
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
        Uri::try_from("/test?name=test"),
        UriQueries::new(None, Some("test".into()), None)
    )]
    #[case(
        Uri::try_from("/test?name=vorona"),
        UriQueries::new(None, Some("vorona".into()), None)
    )]
    #[case(
        Uri::try_from("/test?name=test&resolution=second"),
        UriQueries::new(None, Some("test".into()), Some(Resolution::Second))
    )]
    #[case(
        Uri::try_from("/test?name=test&resolution=minute"),
        UriQueries::new(None, Some("test".into()), Some(Resolution::Minute))
    )]
    #[case(
        Uri::try_from("/test?name=test&resolution=hour"),
        UriQueries::new(None, Some("test".into()), Some(Resolution::Hour))
    )]
    #[case(
        Uri::try_from("/test?name=test&resolution=day"),
        UriQueries::new(None, Some("test".into()), Some(Resolution::Day))
    )]
    fn success_decode_url_params(
        #[case] input: Result<Uri, http::uri::InvalidUri>,
        #[case] expected: UriQueries,
    ) {
        let res: UriQueries = super::super::decode_url_params(&input.unwrap()).unwrap();
        assert_eq!(res, expected);
    }
}
