use std::time;
use std::time::Duration;

use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::Healthcheck;

#[derive(Debug, Clone)]
pub struct HealthcheckService {
    endpoint: String,
    interval: i32,
    agent_uuid: &'static Uuid,
    http_client: reqwest::Client,
    chan: mpsc::UnboundedSender<Healthcheck>,
}

impl HealthcheckService {
    pub fn new(
        endpoint: String,
        interval: i32,
        agent_uuid: &'static Uuid,
        chan: mpsc::UnboundedSender<Healthcheck>,
    ) -> HealthcheckService {
        HealthcheckService {
            endpoint,
            interval,
            chan,
            agent_uuid,
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn spawn(self) -> JoinHandle<()> {
        info!(
            healthcheck.endpoint = self.endpoint,
            msg = "starting healthcheck..."
        );

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
        let pass = match self.ping().await {
            Ok(_) => {
                debug!(healthcheck.endpoint = self.endpoint, status = "pass");
                true
            }
            Err(err) => {
                warn!(check.endpoint = self.endpoint, status = "fail", error = %err);
                false
            }
        };

        // Time it took the healthcheck to run
        let elapsed_time = match start_time.elapsed() {
            Ok(val) => val,
            Err(err) => {
                error!(check.endpoint = self.endpoint, %err, msg = "TIME MOVED BACKWARDS; CLAMPING TO ZERO");
                time::Duration::ZERO
            }
        };

        // Time the healthcheck started
        let since_epoch = start_time.duration_since(time::UNIX_EPOCH).unwrap();

        let result = Healthcheck {
            agent_uuid: self.agent_uuid.to_string(),
            pass,
            endpoint: self.endpoint.clone(),
            start_time: since_epoch.as_secs() as i64,
            elapsed_time: elapsed_time.as_micros() as i64,
        };

        debug!(?result);
        if let Err(err) = self.chan.send(result) {
            error!(check.endpoint = self.endpoint, error = %err);
        };
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

    lazy_static::lazy_static! {
        pub static ref AGENT_UUID: Uuid = Uuid::new_v4();
    }

    async fn test_healthcheck_ping(status_code: u16) -> Result<(), String> {
        let mock_webserver = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/healthcheck"))
            .respond_with(ResponseTemplate::new(status_code))
            .mount(&mock_webserver)
            .await;

        let (sender, _receiver) = mpsc::unbounded_channel::<Healthcheck>();
        let check = HealthcheckService {
            endpoint: format!("{}/healthcheck", &mock_webserver.uri()),
            interval: 1,
            agent_uuid: &AGENT_UUID,
            http_client: reqwest::Client::new(),
            chan: sender,
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
    #[case(101, StatusCode::SWITCHING_PROTOCOLS)]
    #[case(304, StatusCode::NOT_MODIFIED)]
    #[case(404, StatusCode::NOT_FOUND)]
    #[case(500, StatusCode::INTERNAL_SERVER_ERROR)]
    #[tokio::test]
    async fn fail(#[case] status_code: u16, #[case] expected: http::StatusCode) {
        let res = test_healthcheck_ping(status_code).await.unwrap_err();
        assert_eq!(res, expected.to_string());
    }
}
