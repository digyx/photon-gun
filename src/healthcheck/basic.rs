use std::sync::Arc;

use serde::Deserialize;
use sqlx::{Pool, Postgres};
use tracing::{debug, error, info, warn};

use crate::db;

#[derive(Debug, Deserialize)]
pub struct BasicCheckConfig {
    pub name: String,
    // HTTP URL the check will send a GET request to
    pub endpoint: String,
    // Length of time in seconds between checks starting
    pub interval: u64,
}

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
        // Checks will count ALL errors as a failed healthcheck and the messages saved to
        // Postgres and logged via tracing
        //
        // Reqwest Error............String representation of error
        // Status Code is not 2xx...String representation of status code (ex. "404 Not Found")
        // Status Code is 2xx.......Empty string
        let res = match self.ping().await {
            Ok(_) => {
                info!(%self.name, status = "pass");
                (true, String::new())
            }
            Err(err) => {
                warn!(%self.name, status = "fail", error = %err);
                (false, err)
            }
        };

        // Save result in postgres
        if let Err(err) = db::record_healthcheck(&self.db_client, &self.name, res.0, &res.1).await {
            error!(%self.name, msg = "UNABLE TO WRITE TO DATABASE", error = %err);
        }
    }

    async fn ping(&self) -> Result<(), String> {
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
