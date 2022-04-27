use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::{error::Error, time::Duration};

use clap::Parser;
use hyper::service::{make_service_fn, service_fn};
use hyper::Server;
use sqlx::postgres::PgPoolOptions;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info, Level};
use tracing_subscriber::{filter, prelude::*};

use photon_gun::webserver;

#[derive(Debug, Parser)]
struct ClapArgs {
    /// Postgres URI or Connection Parameters
    #[clap(long = "postgres")]
    postgres_uri: String,
    #[clap(long = "min-conn", default_value = "1")]
    min_connections: u32,
    #[clap(long = "max-conn", default_value = "5")]
    max_connections: u32,

    /// Logging level (error, warn, info, debug, trace)
    #[clap(long = "log", default_value = "info")]
    logging_level: tracing::Level,

    /// Enable embedded webserver
    #[clap(short = 's', long = "server")]
    enable_webserver: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli_args = ClapArgs::parse();

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

    // Connect to the database
    let pool = PgPoolOptions::new()
        .min_connections(cli_args.min_connections)
        .max_connections(cli_args.max_connections)
        .connect(&cli_args.postgres_uri)
        .await?;
    if let Err(err) = photon_gun::db::initialize_tables(&pool).await {
        error!(%err);
        panic!("{}", err);
    }

    let pool_arc = Arc::new(pool);

    // Sping up Webserver
    if cli_args.enable_webserver {
        let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
        let db_client = pool_arc.clone();
        let hyper_service = service_fn(move |req| webserver::handler(req, db_client.clone()));

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

    // Spin off healthchecks into their own Tokio task
    // We save the handlers for aborting later if they are removed or to stop the service
    let mut healthcheck_handlers = HashMap::new();

    for healthcheck in photon_gun::db::get_healthchecks(pool_arc).await? {
        let id = healthcheck.get_id();
        info!(healthcheck.id = id, msg = "starting basic check...");

        // Ensures that the tasks runs every two seconds without being affected by the execution
        // time.  This does mean checks can overlap if execution takes too long
        let mut interval =
            tokio::time::interval(Duration::from_secs(healthcheck.get_interval() as u64));

        let task = tokio::task::spawn(async move {
            loop {
                // Temporary workaround
                let check = healthcheck.clone();

                // Tik tok
                // Initial call is passed through immediately
                interval.tick().await;

                // This is done so checks will kick off every second instead of being
                tokio::task::spawn(async move {
                    check.spawn().await;
                });
            }
        });

        healthcheck_handlers.insert(id, task);
    }

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    // Wait for all handlers
    tokio::select! {
        _ = sigint.recv() => info!(msg = "SIGINT received"),
        _ = sigterm.recv() => info!(msg = "SIGTERM received"),
    }

    info!(msg = "Aborting tasks...");
    for handle in healthcheck_handlers {
        handle.1.abort();
    }

    info!(msg = "Tasks stopped.");
    Ok(())
}
