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
        s.serialize_field(
            "elapsed_time",
            &(self.elapsed_time.microseconds as f64 / 1000_f64),
        )?;
        s.serialize_field("pass", &self.pass)?;
        s.serialize_field("message", &self.message)?;
        s.end()
    }
}

pub async fn handle<'a, E>(req: Request<Body>, db_client: E) -> Response<Body>
where
    E: PgExecutor<'a>,
{
    if req.uri().query().is_none() {
        let check_names = match list_check_names(db_client).await {
            Ok(val) => val,
            Err(err) => {
                error!(%err);
                return super::response_with_string_body(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "shit...",
                );
            }
        };

        return Response::new(Body::from(check_names));
    }

    let queries: UriQueries = match super::decode_url_params(req.uri()) {
        Ok(val) => val,
        Err(_) => {
            return super::response_with_string_body(
                StatusCode::BAD_REQUEST,
                "invalid parameters: alphanumeric parameter 'service' is required with optional positive integer parameter 'limit'",
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

async fn list_check_names<'a, E>(db_client: E) -> Result<String, String>
where
    E: PgExecutor<'a>,
{
    let result: Vec<TableNames> = match sqlx::query_as(
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
    .await
    {
        Ok(val) => val,
        Err(err) => {
            error!(%err);
            vec![]
        }
    };

    let table_names: Vec<String> = result
        .iter()
        .map(|a| match db::decode_table_name(&a.table_name) {
            Some(val) => val,
            None => {
                error!("failed to decode healthcheck names");
                "FUCK".into()
            }
        })
        .collect();
    let body = match serde_json::to_string(&table_names) {
        Ok(val) => val,
        Err(err) => {
            error!(%err);
            err.to_string()
        }
    };

    Ok(body)
}
