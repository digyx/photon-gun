use std::sync::Arc;

use hlua::LuaError;
use serde::Deserialize;
use sqlx::{Pool, Postgres};
use tracing::{error, info, warn};

use crate::db;

#[derive(Debug, Deserialize)]
pub struct LuxuryCheckConfig {
    pub name: String,
    // Path to Lua script to be ran for the check
    // Relative paths start in /etc/photon-gun/scripts/
    pub script: String,
    // Length of time in seconds between checks starting
    pub interval: u64,
}

#[derive(Debug)]
pub struct LuxuryCheck {
    name: String,
    script: String,
    db_client: Arc<Pool<Postgres>>,
    handle: tokio::runtime::Handle,
}

impl LuxuryCheck {
    pub fn new(conf: LuxuryCheckConfig, db_client: Arc<Pool<Postgres>>, script: String) -> Self {
        LuxuryCheck {
            name: conf.name,
            script,
            db_client,
            handle: tokio::runtime::Handle::current(),
        }
    }

    pub fn spawn(&self) {
        let res = match self.ping() {
            Ok(msg) => {
                info!(%self.name, status = "pass", %msg);
                (true, msg)
            }
            Err(err) => {
                warn!(%self.name, status = "fail", error = %err);
                (false, err)
            }
        };

        let table_name = self.name.clone();
        let db_client = self.db_client.clone();

        self.handle.spawn(async move {
            if let Err(err) =
                db::record_healthcheck(&db_client, &table_name, res.0, &res.1).await
            {
                error!(service.name = %table_name, msg = "UNABLE TO WRITE TO DATABASE", error = %err);
            }
        });
    }

    fn ping(&self) -> Result<String, String> {
        let mut lua = hlua::Lua::new();
        lua.open_base();
        lua.set("http_get", hlua::function1(http_get));

        match lua.execute::<String>(&self.script) {
            Ok(res) => Ok(res),
            Err(LuaError::ExecutionError(err)) => Err(err),
            Err(err) => {
                error!(error = %err);
                Err(err.to_string())
            }
        }
    }
}

fn http_get(url: String) -> (u16, String) {
    let client = reqwest::blocking::Client::new();
    match client.get(url).send() {
        Ok(res) => {
            let status_code = res.status().as_u16();
            let body = res.text().unwrap();

            (status_code, body)
        }
        Err(err) => (0, err.to_string()),
    }
}
