use hyper::{Body, Request, Response, StatusCode, Uri};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use sqlx::types::chrono;
use sqlx::{FromRow, PgExecutor};
use tracing::{debug, error};

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct SummaryQueries {
    #[serde(alias = "service")]
    service_name: String,
    resolution: Option<SummaryResolution>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum SummaryResolution {
    Second,
    Minute,
    Hour,
    Day,
}

impl std::fmt::Display for SummaryResolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let res = match self {
            SummaryResolution::Second => "second",
            SummaryResolution::Minute => "minute",
            SummaryResolution::Hour => "hour",
            SummaryResolution::Day => "day",
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
    let queries = match decode_url_params(req.uri()) {
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
        queries.resolution.unwrap_or(SummaryResolution::Minute),
        queries.service_name,
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

fn decode_url_params(uri: &Uri) -> Result<SummaryQueries, &'static str> {
    let queries = match uri.query() {
        Some(val) => val,
        None => return Err("No parameters passed when 'service' is required."),
    };

    match serde_qs::from_str(queries) {
        Ok(val) => Ok(val),
        Err(_) => Err("Invalid parameters.  Only 'service' (string) and 'resolution' (second,minute,hour,day) are supported.")
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    impl SummaryQueries {
        fn new(service_name: String, resolution: Option<SummaryResolution>) -> SummaryQueries {
            SummaryQueries {
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
        SummaryQueries::new("test".into(), None)
    )]
    #[case(
        Uri::try_from("/test?service=vorona"),
        SummaryQueries::new("vorona".into(), None)
    )]
    #[case(
        Uri::try_from("/test?service=test&resolution=second"),
        SummaryQueries::new("test".into(), Some(SummaryResolution::Second))
    )]
    #[case(
        Uri::try_from("/test?service=test&resolution=minute"),
        SummaryQueries::new("test".into(), Some(SummaryResolution::Minute))
    )]
    #[case(
        Uri::try_from("/test?service=test&resolution=hour"),
        SummaryQueries::new("test".into(), Some(SummaryResolution::Hour))
    )]
    #[case(
        Uri::try_from("/test?service=test&resolution=day"),
        SummaryQueries::new("test".into(), Some(SummaryResolution::Day))
    )]
    fn success_decode_url_params(
        #[case] input: Result<Uri, http::uri::InvalidUri>,
        #[case] expected: SummaryQueries,
    ) {
        let res = decode_url_params(&input.unwrap()).unwrap();
        assert_eq!(res, expected);
    }
}
