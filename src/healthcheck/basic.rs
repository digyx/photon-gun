use std::sync::Arc;
use std::time;

use sqlx::{Pool, Postgres};
use tracing::{debug, error, info, warn};

use crate::config::BasicCheckConfig;
use crate::db;

#[derive(Debug)]
pub struct BasicCheck {
    name: String,
    endpoint: String,
    db_client: Arc<Pool<Postgres>>,
    http_client: reqwest::Client,
}

impl BasicCheck {
    pub fn new(conf: BasicCheckConfig, db_client: Arc<Pool<Postgres>>) -> Self {
        BasicCheck {
            name: conf.name,
            endpoint: conf.endpoint,
            db_client,
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn spawn(&self) {
        let start_time = time::SystemTime::now();
        // Checks will count ALL non-successes as a failed healthcheck and the messages will be
        // saved to Postgres and logged via tracing
        //
        // Reqwest Error............String representation of error
        // Status Code is not 2xx...String representation of status code (ex. "404 Not Found")
        // Status Code is 2xx.......Empty string
        let res = match self.run().await {
            Ok(_) => {
                info!(%self.name, status = "pass");
                (true, String::new())
            }
            Err(err) => {
                warn!(%self.name, status = "fail", error = %err);
                (false, err)
            }
        };

        let result = super::HealthcheckResult::new(&self.name, res.0, res.1, start_time);

        // Save result in postgres
        if let Err(err) = db::record_healthcheck(&*self.db_client, result).await {
            error!(service.name = %self.name, msg = "UNABLE TO WRITE TO DATABASE", error = %err);
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

    #[tokio::test]
    async fn create_basic_check() {
        let db_client = Arc::new(PgPool::connect_lazy("postgres://localhost/").unwrap());
        let input = BasicCheckConfig {
            name: "test".into(),
            endpoint: "https://test.com".into(),
            interval: 5,
        };
        let expected = BasicCheck {
            name: "test".into(),
            endpoint: "https://test.com".into(),
            db_client: db_client.clone(),
            http_client: reqwest::Client::new(),
        };

        let res = BasicCheck::new(input, db_client.clone());
        assert_eq!(res.name, expected.name);
        assert_eq!(res.endpoint, expected.endpoint);
    }

    async fn test_basic_check_run(status_code: u16) -> Result<(), String> {
        let mock_webserver = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/healthcheck"))
            .respond_with(ResponseTemplate::new(status_code))
            .mount(&mock_webserver)
            .await;

        let check = BasicCheck {
            name: "test".into(),
            endpoint: format!("{}/healthcheck", &mock_webserver.uri()),
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
        test_basic_check_run(status_code).await.unwrap()
    }

    #[rstest]
    #[case(101, StatusCode::SWITCHING_PROTOCOLS.to_string())]
    #[case(304, StatusCode::NOT_MODIFIED.to_string())]
    #[case(404, StatusCode::NOT_FOUND.to_string())]
    #[case(500, StatusCode::INTERNAL_SERVER_ERROR.to_string())]
    #[tokio::test]
    async fn fail(#[case] status_code: u16, #[case] expected: String) {
        let res = test_basic_check_run(status_code).await.unwrap_err();
        assert_eq!(res, expected);
    }
}
