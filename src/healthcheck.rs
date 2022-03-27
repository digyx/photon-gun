use tracing::{info,error,warn};

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
                warn!(status = "fail", endpoint);
                return HealthcheckResult::Fail
            }

            info!(status = "pass", endpoint);
            HealthcheckResult::Pass
        },

        // Reqwest failure
        Err(err) => {
            error!(error = format!("{err}").as_str());
            HealthcheckResult::Error(err.to_string())
        }
    }
}

