use tracing::{info,error,warn};

pub enum HealthcheckResult {
    Pass,
    Fail,
    Error(String),
}

#[tracing::instrument]
pub async fn healthcheck(client: &reqwest::Client, endpoint: &str) -> HealthcheckResult {
    match client.get(endpoint).send().await {
        Ok(res) => {
            // .is_success() includes ALL 200-299 codes
            if !res.status().is_success() {
                warn!(target: "healthcheck", status = "fail");
                return HealthcheckResult::Fail
            }

            info!(target: "healthcheck", status = "pass");
            HealthcheckResult::Pass
        },

        // Reqwest failure
        Err(err) => {
            error!(target: "healthcheck", err = format!("{err}").as_str());
            HealthcheckResult::Error(err.to_string())
        }
    }
}

