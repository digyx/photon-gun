use tracing::debug;

// Pretty simple function.  Make GET request to the endpoint.  Return Ok() is 2xx status code is
// received.  Return Err(error_msg) otherwise.  For non-2xx status codes, returns string
// repesentation of status code (ex. 404 Not Found)
pub async fn basic_check(client: &reqwest::Client, endpoint: &str) -> Result<(), String> {
    let res = match client.get(endpoint).send().await {
        Ok(res) => res,
        Err(err) => return Err(err.to_string()),
    };

    // .is_success() includes ALL 200-299 codes
    if !res.status().is_success() {
        debug!(?res);
        return Err(res.status().to_string());
    }

    debug!(?res);
    debug!(endpoint, status = "pass");
    Ok(())
}
