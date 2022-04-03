use hyper::{Body, Request, Response, StatusCode};
use serde::{Deserialize, Serialize, ser::SerializeStruct};
use sqlx::{postgres, types::chrono, FromRow};
use tracing::error;

#[derive(Debug, Deserialize)]
struct SummaryQueries {
    service_name: String,
    resolution: Option<String>,
}

#[derive(Debug,FromRow)]
struct HealthcheckSummary {
    time_window: chrono::NaiveDateTime,
    pass: i64,
    fail: i64,
}

impl Serialize for HealthcheckSummary {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
            S: serde::Serializer {
        let mut s = serializer.serialize_struct("HealthcheckSummary", 3)?;
        s.serialize_field("time_window", &self.time_window.to_string())?;
        s.serialize_field("pass", &self.pass)?;
        s.serialize_field("fail", &self.fail)?;
        s.end()
    }
}

pub async fn summary(req: Request<Body>, db_client: &postgres::PgPool) -> Response<Body> {
    let raw_query = match req.uri().query() {
        Some(raw_query) => raw_query,
        None => {
            let res = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from("No queries given when 'service' is required."))
                .unwrap();
            return res;
        }
    };

    let queries: SummaryQueries = match serde_qs::from_str(raw_query) {
        Ok(queries) => queries,
        Err(err) => {
            let res = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(err.to_string()))
                .unwrap();
            return res;
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
        queries.resolution.unwrap_or("minute".into()),
        queries.service_name,
    );

    let result: Vec<HealthcheckSummary> =
        match sqlx::query_as(&sql_query).fetch_all(db_client).await {
            Ok(result) => result,
            Err(err) => {
                error!(%err);
                let res = Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(err.to_string()))
                    .unwrap();
                return res;
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
