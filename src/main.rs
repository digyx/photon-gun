use std::time::Duration;

use healthcheck::HealthcheckResult;
use tracing::{error,info,debug};

mod config;
mod healthcheck;
mod db;

#[tokio::main]
async fn main() {
    let cli_args = config::load_cli_args();

    // Enable tracing
    tracing_subscriber::fmt()
        .with_max_level(cli_args.logging_level)
        .init();

    let conf = config::load_config_file(cli_args.config_path);

    // Start the healthchecks
    let mut handlers = vec![];
    for service in conf.basic_checks {
        info!(target: "service_events", service = service.name.as_str(), msg = "Starting...");

        let task = tokio::task::spawn(async move {
            shoot(service).await
        });

        handlers.push(task);
    }

    // Wait for all handlers
    info!("Listening for SIGINT...");
    tokio::signal::ctrl_c().await.expect("failed to listen for event");
    info!("SIGINT Receieved.");

    info!("Aborting...");
    for handle in handlers {
        handle.abort();
    }

    info!("Tasks stopped.");
}

#[tracing::instrument(level = "debug")]
async fn shoot(service: config::BasicCheck) {
    let db_client = match db::DB::new(service.name.clone()).await {
        Ok(client) => client,
        Err(_) => {
            // Postgres error logged in `DB::new` function
            error!(error = "UNABLE TO CONNECT TO DATABASE");
            return
        }
    };

    info!(msg = "starting healthchecks");

    // TODO: Determine if check is simple or complex BEFORE starting the loop
    // currently it does neither, so there's no real issue right now
    let http_client = reqwest::Client::new();
    loop {
        let res = match healthcheck::healthcheck(&http_client, &service.endpoint).await {
            HealthcheckResult::Pass => true,
            HealthcheckResult::Fail => false,
            // Reqwest error logged in `healthcheck` function
            HealthcheckResult::Error(_) => {
                error!(error = "UNABLE TO SEND HTTP REQUEST");
                return
            }
        };

        if db_client.record_healthcheck(res).await.is_err() {
            // Postgres error logged in `record_healthcheck` function
            error!(error = "UNABLE TO WRITE TO DATABASE");
            return
        }

        debug!(msg = "sleeping", duration = service.interval);
        tokio::time::sleep(Duration::from_secs(service.interval)).await;
    }
}

