use std::sync::Arc;
use std::time;

use sqlx::{postgres::types::PgInterval, types::chrono, PgPool};
use tracing::{debug, error, info, warn};

use crate::{db, HealthcheckResult};

#[derive(Debug, Clone)]
pub struct Healthcheck {
    id: i32,
    endpoint: String,
    interval: i32,
    db_client: Arc<PgPool>,
    http_client: reqwest::Client,
}

impl Healthcheck {
    pub(crate) fn new(
        id: i32,
        endpoint: String,
        interval: i32,
        db_client: Arc<PgPool>,
    ) -> Healthcheck {
        Healthcheck {
            id,
            endpoint,
            interval,
            db_client,
            http_client: reqwest::Client::new(),
        }
    }

    pub fn get_id(&self) -> i32 {
        self.id
    }

    pub fn get_interval(&self) -> i32 {
        self.interval
    }

    pub async fn spawn(&self) {
        let start_time = time::SystemTime::now();
        // Checks will count ALL non-successes as a failed healthcheck and the messages will be
        // saved to Postgres and logged via tracing
        //
        // Reqwest Error............String representation of error
        // Status Code is not 2xx...String representation of status code (ex. "404 Not Found")
        // Status Code is 2xx.......Empty string
        let (pass, msg) = match self.run().await {
            Ok(_) => {
                info!(check.id = self.id, status = "pass");
                (true, None)
            }
            Err(err) => {
                warn!(check.id = self.id, status = "fail", error = %err);
                (false, Some(err))
            }
        };

        let since_epoch = start_time.duration_since(time::UNIX_EPOCH).unwrap();
        let elapsed_time = match start_time.elapsed() {
            Ok(val) => val,
            Err(err) => {
                error!(check.id = self.id, %err, msg = "TIME MOVED BACKWARDS; CLAMPING TO ZERO");
                time::Duration::ZERO
            }
        };

        let result = HealthcheckResult {
            check_id: self.id,
            start_time: chrono::NaiveDateTime::from_timestamp(since_epoch.as_secs() as i64, 0),
            elapsed_time: PgInterval {
                months: 0,
                days: 0,
                microseconds: elapsed_time.as_micros() as i64,
            },
            pass,
            message: msg,
        };

        // Save result in postgres
        if let Err(err) = db::insert_healthcheck_result(&self.db_client, result).await {
            error!(check.id = self.id, msg = "UNABLE TO WRITE TO DATABASE", error = %err);
        }
    }

    async fn run(&self) -> Result<(), String> {
        let res = match self.http_client.get(&self.endpoint).send().await {
            Ok(res) => res,
            Err(err) => return Err(err.to_string()),
        };

        // .is_success() includes ALL 200-299 codes
        if !res.status().is_success() {
            debug!(?res);
            return Err(res.status().to_string());
        }

        debug!(?res);
        debug!(%self.endpoint, status = "pass");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rstest::rstest;

    use hyper::StatusCode;
    use sqlx::PgPool;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn test_healthcheck_run(status_code: u16) -> Result<(), String> {
        let mock_webserver = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/healthcheck"))
            .respond_with(ResponseTemplate::new(status_code))
            .mount(&mock_webserver)
            .await;

        let check = Healthcheck {
            id: 0,
            endpoint: format!("{}/healthcheck", &mock_webserver.uri()),
            interval: 1,
            db_client: Arc::new(PgPool::connect_lazy("postgres://localhost/").unwrap()),
            http_client: reqwest::Client::new(),
        };

        check.run().await
    }

    #[rstest]
    #[case(200)]
    #[case(201)]
    #[case(202)]
    #[tokio::test]
    async fn success(#[case] status_code: u16) {
        test_healthcheck_run(status_code).await.unwrap()
    }

    #[rstest]
    #[case(101, StatusCode::SWITCHING_PROTOCOLS.to_string())]
    #[case(304, StatusCode::NOT_MODIFIED.to_string())]
    #[case(404, StatusCode::NOT_FOUND.to_string())]
    #[case(500, StatusCode::INTERNAL_SERVER_ERROR.to_string())]
    #[tokio::test]
    async fn fail(#[case] status_code: u16, #[case] expected: String) {
        let res = test_healthcheck_run(status_code).await.unwrap_err();
        assert_eq!(res, expected);
    }
}
