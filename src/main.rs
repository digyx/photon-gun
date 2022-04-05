use std::convert::Infallible;
use std::fs;
use std::net::SocketAddr;
use std::{error::Error, sync::Arc, time::Duration};

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use sqlx::postgres::{self, PgPoolOptions};
use tokio::signal::unix::{signal, SignalKind};
use tracing::{debug, error, info, Level};
use tracing_subscriber::{filter, prelude::*};

mod config;
mod db;
mod healthcheck;
mod webserver;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli_args = config::load_cli_args();

    // sqlx::query logs ALL sql queries by defualt to Level::INFO
    // This is excessive, so this will only allow WARN and ERROR logs from sqlx
    let filter = filter::Targets::new()
        .with_target("photon_gun", cli_args.logging_level)
        .with_target("sqlx::query", Level::WARN);

    // Enable tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .init();

    let conf = config::load_config_file(&cli_args.config_path);
    let pool = PgPoolOptions::new()
        .min_connections(conf.postgres.min_connections)
        .max_connections(conf.postgres.max_connections)
        .connect(&conf.postgres.uri)
        .await?;
    let pool_arc = Arc::new(pool);

    if cli_args.enable_webserver {
        let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
        let db_client = pool_arc.clone();
        let hyper_service = service_fn(move |req| webserver_handler(req, db_client.clone()));

        let service = make_service_fn(move |_conn| {
            let hyper_service = hyper_service.clone();
            async move { Ok::<_, Infallible>(hyper_service.clone()) }
        });

        tokio::task::spawn(async move {
            let server = Server::bind(&addr).serve(service);
            info!("Listening on http://{}", addr);

            if let Err(err) = server.await {
                error!(%err, msg = "WEBSERVER CRASH.");
                panic!("{}", err);
            }
        });
    }

    // Spin off basic check off into its own Tokio task
    // We save the handlers for aborting later, if necessary
    let mut handlers = vec![];
    for service in conf.basic_checks {
        info!(%service.name, msg = "starting basic check...");

        // Increment the RC on the PgPool ARC to deal with the move
        let db_client_arc = pool_arc.clone();
        // Create the database table for the basic check
        // Every basic check gets its own table
        db::create_healthcheck_table(&db_client_arc, &service.name).await?;

        // Ensures that the tasks runs every two seconds without being affected by the execution
        // time.  This does mean checks can overlap if execution takes too long
        let mut interval = tokio::time::interval(Duration::from_secs(service.interval));

        let basic_check_arc = Arc::new(healthcheck::BasicCheck::new(service));

        let task = tokio::task::spawn(async move {
            debug!(?basic_check_arc);

            loop {
                // Tik tok
                // Initial call is passed through immediately
                interval.tick().await;
                let basic_check = basic_check_arc.clone();
                let db_client = db_client_arc.clone();

                // This is done so checks will kick off every second instead of being
                tokio::task::spawn(async move {
                    basic_check.spawn(db_client.clone()).await;
                });
            }
        });

        handlers.push(task);
    }

    // The majority of this logic is the same as the basic check, so I'll only elaborate on the
    // stuff unique to this loop
    for service in conf.luxury_checks {
        info!(%service.name, msg = "starting luxury check...");

        let db_client = pool_arc.clone();
        db::create_healthcheck_table(&db_client, &service.name).await?;

        let mut interval = tokio::time::interval(Duration::from_secs(service.interval));

        // Script paths can be relative to the config dir; absolute paths always start with '/'
        let script_path = match service.script_path.starts_with('/') {
            true => service.script_path,
            // ex. /etc/photon-gun/scripts/script.lua
            // ex. examples/scripts/script.lua
            false => format!("{}/{}", &cli_args.script_dir, &service.script_path),
        };

        // If a lua script can't be read in, then the check needs to fail immediately
        let lua_script = match fs::read_to_string(&script_path) {
            Ok(contents) => contents,
            Err(err) => {
                error!(%service.name, %err, %script_path, msg = "FAILED TO START HEALTHCHECK");
                // We don't want to crash the entire program, though, so continue the loop
                continue;
            }
        };

        let luxury_check_arc = Arc::new(healthcheck::LuxuryCheck::new(
            service.name,
            db_client,
            lua_script,
        ));

        let task = tokio::task::spawn(async move {
            debug!(?luxury_check_arc);

            loop {
                interval.tick().await;
                let luxury_check = luxury_check_arc.clone();

                // Lua does not play well with async-await
                tokio::task::spawn_blocking(move || {
                    luxury_check.spawn();
                });
            }
        });

        handlers.push(task);
    }

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    // Wait for all handlers
    tokio::select! {
        _ = sigint.recv() => info!(msg = "SIGINT received"),
        _ = sigterm.recv() => info!(msg = "SIGTERM received"),
    }

    info!(msg = "Aborting tasks...");
    for handle in handlers {
        handle.abort();
    }

    info!(msg = "Tasks stopped.");
    Ok(())
}

async fn webserver_handler(
    req: Request<Body>,
    db_client: Arc<postgres::PgPool>,
) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => Ok(Response::new(Body::from("Pew pew"))),
        (&Method::GET, "/healthcheck") => Ok(Response::new(Body::from("Ok"))),
        (&Method::GET, "/summary") => Ok(webserver::summary(req, &db_client).await),
        _ => {
            let res = Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("404 - Page Not Found"))
                .unwrap();
            Ok(res)
        }
    }
}
