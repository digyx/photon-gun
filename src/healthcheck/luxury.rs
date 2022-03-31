use std::sync::Arc;

use rlua::{Lua, StdLib};
use serde::Deserialize;
use sqlx::{Pool, Postgres};
use tracing::{debug, error, info, warn};

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
        let lua = Lua::new();

        lua.load_from_std_lib(StdLib::BASE).unwrap();

        // Load Photon module into lua context
        // This is a table with various Rust functions that can be accessed in lua through the
        // photon table
        //
        // Planned functions:
        //  - HTTP
        //      - GET (implemented)
        //      - POST w/ Body
        //      - Custom Request w/ Method, Body, Headers
        //  - JSON (probs custom parser)
        //      - Encode from Table
        //      - Decode to Table
        match lua.context(|ctx| {
            let http_table = ctx.create_table()?;
            http_table.set("get", ctx.create_function(http_get)?)?;

            let photon_table = ctx.create_table()?;
            photon_table.set("http", http_table)?;

            ctx.globals().set("photon", photon_table)
        }) {
            Ok(_) => debug!("Photon module loaded into Lua context."),
            Err(err) => {
                error!(%self.name, msg = "Could not load Photon module into Lua environment.", %err);
                return Err(err.to_string());
            }
        };

        lua.context(|ctx| match ctx.load(&self.script).eval() {
            Ok(res) => Ok(res),
            Err(err) => {
                error!(%err);
                Err(err.to_string())
            }
        })
    }
}

// Send basic HTTP GET request
// Returns status code and response body
fn http_get(_ctx: rlua::Context, url: String) -> Result<(u16, String), rlua::Error> {
    match reqwest::blocking::get(url) {
        Ok(res) => {
            let status_code = res.status().as_u16();
            let body = res.text().unwrap();

            Ok((status_code, body))
        }
        Err(err) => Err(rlua::Error::ExternalError(Arc::new(err))),
    }
}
