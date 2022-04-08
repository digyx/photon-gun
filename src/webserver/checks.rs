use std::time::Duration;

use hyper::{Body, Request, Response};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use sqlx::types::chrono;
use sqlx::{FromRow, PgExecutor};
use tracing::error;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct UriQueries {
    #[serde(alias = "service")]
    service_name: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, FromRow)]
struct Healthcheck {
    start_time: chrono::NaiveDateTime,
    elapsed_time: Duration,
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
        s.serialize_field("elapsed_time", &self.pass)?;
        s.serialize_field("pass", &self.pass)?;
        s.serialize_field("message", &self.pass)?;
        s.end()
    }
}

pub async fn handle<'a, E>(_req: Request<Body>, db_client: E) -> Response<Body>
where
    E: PgExecutor<'a>,
{
    let result: Vec<TableNames> =
        match sqlx::query_as(
            "SELECT table_name FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE'"
        )
        .fetch_all(db_client)
        .await {
            Ok(val) => val,
            Err(err) => {
                error!(%err);
                vec![]
            }
        };

    let table_names: Vec<&str> = result.iter().map(|a| a.table_name.as_str()).collect();
    let body = match serde_json::to_string(&table_names) {
        Ok(val) => val,
        Err(err) => {
            error!(%err);
            err.to_string()
        }
    };

    Response::new(Body::from(body))
}
