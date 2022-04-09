use std::sync::Arc;
use std::{convert::Infallible, fmt::Debug};

use hyper::{Body, Method, Request, Response, StatusCode, Uri};
use serde::Deserialize;
use sqlx::postgres;
use tracing::debug;

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

// ==================== Common Utility Functions ====================
fn response_with_string_body(status: StatusCode, msg: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::from(msg))
        .unwrap()
}

fn decode_url_params<'de, T: Deserialize<'de> + Debug>(uri: &'de Uri) -> Result<T, &'static str> {
    let queries = match uri.query() {
        Some(val) => val,
        None => return Err("no parameters given when required."),
    };

    debug!(%queries);

    match serde_qs::from_str(queries) {
        Ok(val) => {
            debug!(parameters = ?val);
            Ok(val)
        }
        Err(_) => Err("missing required parameters or invalid parameters given."),
    }
}
