use std::convert::Infallible;
use std::sync::Arc;

use hyper::{Body, Method, Request, Response, StatusCode};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use sqlx::types::chrono;
use sqlx::{postgres, FromRow, PgExecutor};
use tracing::{debug, error};

#[derive(Debug, Deserialize)]
struct SummaryQueries {
    service_name: String,
    resolution: Option<SummaryResolution>,
}

#[derive(Debug, Deserialize)]
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

pub async fn handler(
    req: Request<Body>,
    db_client: Arc<postgres::PgPool>,
) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => Ok(Response::new(Body::from("Pew pew"))),
        (&Method::GET, "/healthcheck") => Ok(Response::new(Body::from("Ok"))),
        (&Method::GET, "/summary") => Ok(handle_summary(req, &*db_client).await),
        _ => {
            let res = Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("404 - Page Not Found"))
                .unwrap();
            Ok(res)
        }
    }
}

async fn handle_summary<'a, E>(req: Request<Body>, db_client: E) -> Response<Body>
where
    E: PgExecutor<'a>,
{
    let raw_query = match req.uri().query() {
        Some(raw_query) => raw_query,
        None => {
            return response_with_string_body(
                StatusCode::BAD_REQUEST,
                "No parameters given when 'service' is required.",
            );
        }
    };

    let queries: SummaryQueries = match serde_qs::from_str(raw_query) {
        Ok(queries) => queries,
        Err(err) => {
            debug!(%err, msg = "Invalid URL paramters given.");
            return response_with_string_body(
                StatusCode::BAD_REQUEST,
                "Invalid URL parameters.  See the docs at github.com/digyx/photon-gun for valid URL parameters.",
            );
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
                return response_with_string_body(
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

fn response_with_string_body(status: StatusCode, msg: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::from(msg))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
