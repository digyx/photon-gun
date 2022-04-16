use hyper::{Body, Request, Response, StatusCode};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use sqlx::postgres::types::PgInterval;
use sqlx::types::chrono;
use sqlx::{FromRow, PgPool};
use tracing::error;


#[derive(Debug, Deserialize, PartialEq, Eq)]
struct UriQueries {
    #[serde(alias = "service")]
    id: Option<i32>,
    name: Option<String>,
    limit: Option<i32>,
}

#[allow(dead_code)]
#[derive(Debug, FromRow)]
struct Healthcheck {
    start_time: chrono::NaiveDateTime,
    elapsed_time: PgInterval,
    pass: bool,
}

#[derive(Debug, Serialize, FromRow)]
struct BasicCheck {
    check_id: i32,
    name: Option<String>,
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
        s.end()
    }
}

pub async fn handle(req: Request<Body>, db_client: &PgPool) -> Response<Body> {
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
                "invalid parameters: parameter 'name' or 'id' is required",
            )
        }
    };

    let sql_query = "
        SELECT
            start_time,
            elapsed_time,
            pass,
        FROM basic_check_results
        WHERE check_id=$1
            OR name=$2
        ORDER BY id DESC
        LIMIT $3
        ";


    let result: Vec<Healthcheck> = match sqlx::query_as(sql_query)
        .bind(queries.id)
        .bind(queries.name)
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

async fn list_check_names(db_client: &PgPool) -> Result<String, &'static str> {
    let rows = sqlx::query_as("SELECT check_id, name FROM basic_checks")
    .fetch_all(db_client)
    .await;

    // 'rows' variable exists for reabability
    let result: Vec<BasicCheck> = match rows {
        Ok(val) => val,
        Err(err) => {
            error!(%err);
            vec![]
        }
    };

    let body = match serde_json::to_string(&result) {
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
        fn new(id: Option<i32>, name: Option<String>, limit: Option<i32>) -> UriQueries {
            UriQueries {
                id,
                name,
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
        };
        let expected = "{\"start_time\":\"2022-04-01 00:00:00\",\"elapsed_time\":42,\"pass\":true}";

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
        Uri::try_from("/test?name=test&limit=10"),
        UriQueries::new(None, Some("test".into()), Some(10))
    )]
    fn success_decode_url_params(
        #[case] input: Result<Uri, http::uri::InvalidUri>,
        #[case] expected: UriQueries,
    ) {
        let res: UriQueries = super::super::decode_url_params(&input.unwrap()).unwrap();
        assert_eq!(res, expected);
    }
}
