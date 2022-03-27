use tracing::{debug, warn};

pub async fn basic_check(client: &reqwest::Client, endpoint: &str) -> Result<bool, reqwest::Error> {
    let res = client.get(endpoint).send().await?;

    // .is_success() includes ALL 200-299 codes
    if !res.status().is_success() {
        warn!(endpoint, status = "fail", status_code = %res.status());
        debug!(?res);
        return Ok(false);
    }

    debug!(?res);
    debug!(endpoint, status = "pass");
    Ok(true)
}
