use std::{str::FromStr, sync::Arc};

use rlua::StdLib;
use sqlx::{Pool, Postgres};
use tracing::{debug, error, info, warn};

use crate::db;

#[derive(Debug)]
pub struct LuxuryCheck {
    name: String,
    // Actual Lua script contents
    script: String,
    db_client: Arc<Pool<Postgres>>,
    handle: tokio::runtime::Handle,
}

impl LuxuryCheck {
    pub fn new(name: String, db_client: Arc<Pool<Postgres>>, script: String) -> Self {
        LuxuryCheck {
            name,
            script,
            db_client,
            handle: tokio::runtime::Handle::current(),
        }
    }

    // The plan is to move the spawn function out of here since it doesn't follow the "functional
    // core, imperative shell" principle.  Am I being idealisitc?  Maybe, but the library is small
    // enough for me to be so.
    pub fn spawn(&self) {
        let res = match self.run() {
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

    fn run(&self) -> Result<String, String> {
        let lua = rlua::Lua::new();

        lua.load_from_std_lib(StdLib::BASE).unwrap();
        lua.load_from_std_lib(StdLib::TABLE).unwrap();
        lua.load_from_std_lib(StdLib::STRING).unwrap();
        lua.load_from_std_lib(StdLib::UTF8).unwrap();
        lua.load_from_std_lib(StdLib::MATH).unwrap();

        // Load Photon module into lua context
        // This is a table with various Rust functions that can be accessed in lua through the
        // photon table
        //
        // Since Lua states in rlua can't be cloned or shared between threads, the state must be
        // remade every time, which does incur overhead.
        //
        // Planned functions:
        //  - HTTP
        //      - GET (implemented)
        //      - POST w/ Body (implemented)
        //      - Custom Request w/ Method, Body (implemented)
        //      - Above but with headers, auth, etc. (hopefully all reqwest options)
        //  - JSON (probs custom parser)
        //      - Encode from Table
        //      - Decode to Table
        match lua.context(|ctx| {
            let http_table = ctx.create_table()?;
            http_table.set("request", ctx.create_function(http_request)?)?;
            http_table.set("get", ctx.create_function(http_get)?)?;
            http_table.set("post", ctx.create_function(http_post)?)?;

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

// Send an HTTP request
// Returns status code and response body
// Blocking is used since Lua and async Rust don't play nicely together
fn http_request(
    _ctx: rlua::Context,
    (url, body, method_string): (String, String, String),
) -> Result<(u16, String), rlua::Error> {
    let method = match reqwest::Method::from_str(&method_string) {
        Ok(method) => method,
        Err(err) => return Err(rlua::Error::ExternalError(Arc::new(err))),
    };

    let request = reqwest::blocking::Client::new()
        .request(method, url)
        .body(body);

    match request.send() {
        Ok(res) => {
            let status_code = res.status().as_u16();
            let body = res.text().unwrap();

            Ok((status_code, body))
        }
        Err(err) => Err(rlua::Error::ExternalError(Arc::new(err))),
    }
}

fn http_get(ctx: rlua::Context, url: String) -> Result<(u16, String), rlua::Error> {
    http_request(ctx, (url, "".into(), "GET".into()))
}

fn http_post(
    ctx: rlua::Context,
    (url, body): (String, String),
) -> Result<(u16, String), rlua::Error> {
    http_request(ctx, (url, body, "POST".into()))
}
