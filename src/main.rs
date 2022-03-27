use std::{time::Duration, error::Error, sync::Arc};

use tracing::{error,warn,info,debug};
use tokio::signal::unix::{signal,SignalKind};
use sqlx::postgres::PgPool;

mod config;
mod healthcheck;
mod db;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli_args = config::load_cli_args();

    // Enable tracing
    tracing_subscriber::fmt()
        .with_max_level(cli_args.logging_level)
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
                let is_success = match healthcheck::basic_check(&http_client, &service.endpoint).await {
                    Ok(is_success) => {
                        if !is_success {
                            warn!(%service.name, status = "fail");
                        } else {
                            info!(%service.name, status = "is_success");
                        }

                        is_success
                    },
                    Err(err) => {
                        error!(error = %err);
                        false
                    }
                };

                if let Err(err) = db::record_basic_check(&db_client, &service.name, is_success).await {
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

