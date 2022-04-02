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
        // Checks will count ALL errors as a failed healthcheck and the messages saved to
        // Postgres and logged via tracing
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
        if let Err(err) = db::record_healthcheck(&self.db_client, result).await {
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
