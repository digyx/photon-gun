use std::convert::Infallible;
use std::sync::Arc;

use hyper::{Body, Method, Request, Response, StatusCode};
use sqlx::postgres;

mod summary;

pub async fn handler(
    req: Request<Body>,
    db_client: Arc<postgres::PgPool>,
) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => Ok(Response::new(Body::from("Pew pew"))),
        (&Method::GET, "/healthcheck") => Ok(Response::new(Body::from("Ok"))),
        (&Method::GET, "/summary") => Ok(summary::handle(req, &*db_client).await),
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
