use std::{error::Error, sync::Arc, time::Duration};

use sqlx::postgres::PgPool;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber::{filter, prelude::*};

mod config;
mod db;
mod healthcheck;

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

    let conf = config::load_config_file(cli_args.config_path);
    let pool = PgPool::connect(&conf.postgres_uri).await?;
    let pool_arc = Arc::new(pool);

    // Start the basic checks
    let mut handlers = vec![];
    for service in conf.basic_checks {
        info!(%service.name, msg = "spawning...");

        // Increment the RC on the PgPool ARC to deal with the move
        let db_client = pool_arc.clone();
        // Create the database table for the basic check
        // Every basic check gets its own table
        db::create_basic_check_table(&db_client, &service.name).await?;

        // Each task gets its own reqwest client to re-use existing connections
        let http_client = reqwest::Client::new();
        // Ensures that the tasks runs every two seconds without being affected by the execution
        // time.  This does mean checks can overlap if execution takes too long
        let mut interval = tokio::time::interval(Duration::from_secs(service.interval));

        let task = tokio::task::spawn(async move {
            info!(%service.name, msg = "starting basic checks");
            debug!(?service);
            loop {
                // Tik tok
                // Initial call is passed through immediately
                interval.tick().await;

                // Check will only log a success or failure when an HTTP response is received.
                // Reqwest errors are not counted as they're not representative of an actual
                // healthcheck
                let is_success =
                    match healthcheck::basic_check(&http_client, &service.endpoint).await {
                        Ok(is_success) => {
                            if !is_success {
                                warn!(%service.name, status = "fail");
                            } else {
                                info!(%service.name, status = "pass");
                            }

                            is_success
                        }
                        Err(err) => {
                            error!(error = %err);
                            false
                        }
                    };

                if let Err(err) =
                    db::record_basic_check(&db_client, &service.name, is_success).await
                {
                    error!(%service.name, msg = "UNABLE TO WRITE TO DATABASE", error = %err);
                }
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
