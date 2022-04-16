use std::{str::FromStr, sync::Arc, time};

use rlua::StdLib;
use sqlx::{Pool, Postgres};
use tracing::{debug, error, info, warn};

use crate::db;

#[derive(Debug)]
pub struct LuaCheck {
    id: i32,
    // Actual Lua script contents
    script: String,
    db_client: Arc<Pool<Postgres>>,
}

impl LuaCheck {
    pub fn new(id: i32, db_client: Arc<Pool<Postgres>>, script: String) -> Self {
        LuaCheck {
            id,
            script,
            db_client,
        }
    }

    // Run the healthcheck
    pub fn spawn(&self) {
        let start_time = time::SystemTime::now();
        let (pass, msg) = match self.run() {
            Ok(msg) => {
                info!(check.id = self.id, status = "pass", %msg);
                (true, msg)
            }
            Err(err) => {
                warn!(check.id = self.id, status = "fail", error = %err);
                (false, err)
            }
        };

        let db_client = self.db_client.clone();
        let result =
            super::new_healthcheck_result(self.id, pass, Some(msg), start_time, start_time.elapsed());

        tokio::runtime::Handle::current().spawn(async move {
            if let Err(err) =
                db::insert_basic_check_result(&db_client, result).await
            {
                error!(msg = "UNABLE TO WRITE TO DATABASE", error = %err);
            }
        });
    }

    fn run(&self) -> Result<String, String> {
        let lua = rlua::Lua::new();

        // These libs are tentatively chosen.  Lua scripts should not have access to the underlying
        // system unless absolutely necessary, so those stdlibs are not loaded
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
                error!(check.id = self.id, msg = "Could not load Photon module into Lua environment.", %err);
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

#[cfg(test)]
mod tests {
    use std::fs;

    use sqlx::PgPool;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn setup_http_get_test(script_path: &str, status_code: u16) -> (MockServer, LuaCheck) {
        let mock_webserver = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/healthcheck"))
            .respond_with(ResponseTemplate::new(status_code))
            .mount(&mock_webserver)
            .await;

        let script = fs::read_to_string(script_path).unwrap();
        let check = LuaCheck {
            id: 0,
            script: script.replace("WIREMOCK_URI", &mock_webserver.uri()),
            // Since we don't use the DB in these tests, we can just lazy connect to any URI
            db_client: Arc::new(PgPool::connect_lazy("postgres://localhost/").unwrap()),
        };

        (mock_webserver, check)
    }

    #[tokio::test]
    async fn success_http_get() {
        let (_mock_webserver, check) =
            setup_http_get_test("example/scripts/test_http_get.lua", 200).await;

        tokio::runtime::Handle::current()
            .spawn_blocking(move || {
                assert!(check.run().is_ok());
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn fail_http_get() {
        let (_mock_webserver, check) =
            setup_http_get_test("example/scripts/test_http_get.lua", 404).await;

        tokio::runtime::Handle::current()
            .spawn_blocking(move || {
                assert!(check.run().is_err());
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn success_http_get_with_msg() {
        let (_mock_webserver, check) =
            setup_http_get_test("example/scripts/test_http_get.lua", 200).await;

        tokio::runtime::Handle::current()
            .spawn_blocking(move || {
                assert_eq!("Alles gut", check.run().unwrap());
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn fail_http_get_with_msg() {
        let (_mock_webserver, check) =
            setup_http_get_test("example/scripts/test_http_get.lua", 404).await;

        tokio::runtime::Handle::current()
            .spawn_blocking(move || {
                assert_eq!(
                    "\
                    runtime error: This failed\
                    \nstack traceback:\
                    \n\t[C]: in ?\
                    \n\t[C]: in function 'error'\
                    \n\t[string \"?\"]:7: in main chunk",
                    check.run().unwrap_err()
                );
            })
            .await
            .unwrap();
    }
}
