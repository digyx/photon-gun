use std::time;
use std::{sync::Arc, time::Duration};

use sqlx::PgPool;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::{db, Healthcheck, HealthcheckResult};

#[derive(Debug, Clone)]
pub struct HealthcheckService {
    id: i32,
    endpoint: String,
    interval: i32,
    db_client: Arc<PgPool>,
    http_client: reqwest::Client,
}

impl HealthcheckService {
    pub fn new(check: Healthcheck, db_client: Arc<PgPool>) -> HealthcheckService {
        HealthcheckService {
            id: check.id,
            endpoint: check.endpoint,
            interval: check.interval,
            db_client,
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn spawn(self) -> JoinHandle<()> {
        info!(healthcheck.id = self.id, msg = "starting healthcheck...");

        // Ensures that the tasks runs at every interval without being affected by the execution
        // time.  This does mean checks can overlap if execution takes too long
        let mut interval = tokio::time::interval(Duration::from_secs(self.interval as u64));

        tokio::task::spawn(async move {
            loop {
                let check = self.clone();
                // Tik tok, wait for the next interval
                // Initial call is passed through immediately
                interval.tick().await;

                // This is done so checks will kick off every second instead of being
                tokio::task::spawn(async move {
                    check.run().await;
                });
            }
        })
    }

    async fn run(&self) {
        let start_time = time::SystemTime::now();
        // Checks will count ALL non-successes as a failed healthcheck and the messages will be
        // saved to Postgres and logged via tracing
        //
        // Reqwest Error............String representation of error
        // Status Code is not 2xx...String representation of status code (ex. "404 Not Found")
        // Status Code is 2xx.......Empty string
        let (pass, message) = match self.ping().await {
            Ok(_) => {
                debug!(check.id = self.id, status = "pass");
                (true, None)
            }
            Err(err) => {
                warn!(check.id = self.id, status = "fail", error = %err);
                (false, Some(err))
            }
        };

        // Time it took the healthcheck to run
        let elapsed_time = match start_time.elapsed() {
            Ok(val) => val,
            Err(err) => {
                error!(check.id = self.id, %err, msg = "TIME MOVED BACKWARDS; CLAMPING TO ZERO");
                time::Duration::ZERO
            }
        };

        // Time the healthcheck started
        let since_epoch = start_time.duration_since(time::UNIX_EPOCH).unwrap();

        let result = HealthcheckResult {
            id: 0, // This is not passed into the SQL query
            check_id: self.id,
            start_time: since_epoch.as_secs() as i64,
            elapsed_time: elapsed_time.as_micros() as i64,
            pass,
            message,
        };

        debug!(?result);

        // Save result in postgres
        if let Err(err) = db::insert_healthcheck_result(&self.db_client, result).await {
            error!(check.id = self.id, %err, msg = "UNABLE TO WRITE TO DATABASE");
        }
    }

    async fn ping(&self) -> Result<(), String> {
        let res = match self.http_client.get(&self.endpoint).send().await {
            Ok(res) => res,
            Err(err) => return Err(err.to_string()),
        };

        // .is_success() includes ALL 200-299 codes
        debug!(?res);
        if !res.status().is_success() {
            return Err(res.status().to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use http::StatusCode;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn test_healthcheck_ping(status_code: u16) -> Result<(), String> {
        let mock_webserver = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/healthcheck"))
            .respond_with(ResponseTemplate::new(status_code))
            .mount(&mock_webserver)
            .await;

        let check = HealthcheckService {
            id: 0,
            endpoint: format!("{}/healthcheck", &mock_webserver.uri()),
            interval: 1,
            db_client: Arc::new(PgPool::connect_lazy("postgres://localhost/").unwrap()),
            http_client: reqwest::Client::new(),
        };

        check.ping().await
    }

    #[rstest]
    #[case(200)]
    #[case(201)]
    #[case(202)]
    #[tokio::test]
    async fn success(#[case] status_code: u16) {
        test_healthcheck_ping(status_code).await.unwrap()
    }

    #[rstest]
    #[case(101, StatusCode::SWITCHING_PROTOCOLS.to_string())]
    #[case(304, StatusCode::NOT_MODIFIED.to_string())]
    #[case(404, StatusCode::NOT_FOUND.to_string())]
    #[case(500, StatusCode::INTERNAL_SERVER_ERROR.to_string())]
    #[tokio::test]
    async fn fail(#[case] status_code: u16, #[case] expected: String) {
        let res = test_healthcheck_ping(status_code).await.unwrap_err();
        assert_eq!(res, expected);
    }
}
