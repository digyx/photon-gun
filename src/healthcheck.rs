use tracing::{error,warn,debug};

pub enum HealthcheckResult {
    Pass,
    Fail,
    Error(String),
}

pub async fn healthcheck(client: &reqwest::Client, endpoint: &str) -> HealthcheckResult {
    match client.get(endpoint).send().await {
        Ok(res) => {
            // .is_success() includes ALL 200-299 codes
            if !res.status().is_success() {
                warn!(endpoint, status = "fail", status_code = %res.status());
                debug!(?res);
                return HealthcheckResult::Fail
            }

            debug!(?res);
            debug!(endpoint, status = "pass");
            HealthcheckResult::Pass
        },

        // Reqwest failure
        Err(err) => {
            error!(error = %err);
            HealthcheckResult::Error(err.to_string())
        }
    }
}

