use std::convert::Infallible;
use std::sync::Arc;

use hyper::{Body, Method, Request, Response, StatusCode, Uri};
use serde::Deserialize;
use sqlx::postgres;

mod checks;
mod summary;

pub async fn handler(
    req: Request<Body>,
    db_client: Arc<postgres::PgPool>,
) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => Ok(Response::new(Body::from("Pew pew"))),
        (&Method::GET, "/healthcheck") => Ok(Response::new(Body::from("Ok"))),
        (&Method::GET, "/summary") => Ok(summary::handle(req, &*db_client).await),
        (&Method::GET, "/checks") => Ok(checks::handle(req, &*db_client).await),
        _ => {
            let res = response_with_string_body(StatusCode::NOT_FOUND, "404 - Page Not Found");
            Ok(res)
        }
    }
}

fn response_with_string_body(status: StatusCode, msg: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::from(msg))
        .unwrap()
}

fn decode_url_params<'de, T: Deserialize<'de>>(uri: &'de Uri) -> Result<T, &'static str> {
    let queries = match uri.query() {
        Some(val) => val,
        None => return Err("No parameters passed when 'service' is required."),
    };

    match serde_qs::from_str(queries) {
        Ok(val) => Ok(val),
        Err(_) => Err("Invalid parameters.  Only 'service' (string) and 'resolution' (second,minute,hour,day) are supported.")
    }
}
