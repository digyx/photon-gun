use std::{time::Duration, error::Error};

use tracing::{error,warn,info,debug};
use tokio::signal::unix::{signal,SignalKind};

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

    // Start the basic checks
    let mut handlers = vec![];
    for service in conf.basic_checks {
        info!(%service.name, msg = "spawning...");

        let task = tokio::task::spawn(async move {
            let db_client = match db::DB::new(service.name.clone()).await {
                Ok(client) => client,
                Err(_) => {
                    // Postgres error logged in `DB::new` function
                    error!(error = "UNABLE TO CONNECT TO DATABASE");
                    return
                }
            };

            let http_client = reqwest::Client::new();
            let mut interval = tokio::time::interval(Duration::from_secs(service.interval));

            info!(%service.name, msg = "starting basic checks");
            debug!(?service);
            loop {
                interval.tick().await;

                match healthcheck::basic_check(&http_client, &service.endpoint).await {
                    Ok(success) => {
                        if let Err(err) = db_client.record_basic_check(success).await {
                            error!(%service.name, msg = "UNABLE TO WRITE TO DATABASE", error = %err);
                        }

                        if !success {
                            warn!(%service.name, status = "fail");
                            continue
                        }

                        info!(%service.name, status = "pass");
                    },
                    Err(err) => {
                        error!(error = %err);
                        continue
                    }
                }

            }
        });

        handlers.push(task);
    }

    // Wait for all handlers
    info!(msg = "Listening for SIGINT...");
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

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

