use std::time::Duration;

use healthcheck::HealthcheckResult;
use tracing::{error,warn,info,debug};

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
        info!(%service.name, msg = "spawning...");

        let task = tokio::task::spawn(async move {
            shoot(service).await
        });

        handlers.push(task);
    }

    // Wait for all handlers
    info!(msg = "Listening for SIGINT...");
    tokio::signal::ctrl_c().await.expect("failed to listen for event");
    info!(msg = "SIGINT Receieved.");

    info!(msg = "Aborting tasks...");
    for handle in handlers {
        handle.abort();
    }

    info!(msg = "Tasks stopped.");
}

#[tracing::instrument(level = "trace")]
async fn shoot(service: config::BasicCheck) {
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

        let res = match healthcheck::healthcheck(&http_client, &service.endpoint).await {
            HealthcheckResult::Pass => {
                info!(%service.name, status = "pass");
                true
            },
            HealthcheckResult::Fail => {
                warn!(%service.name, status = "fail");
                false
            },
            // Reqwest error logged in `healthcheck` function
            HealthcheckResult::Error(_) => {
                error!(%service.name, error = "UNABLE TO SEND HTTP REQUEST");
                return
            }
        };

        if db_client.record_healthcheck(res).await.is_err() {
            // Postgres error logged in `record_healthcheck` function
            error!(%service.name, error = "UNABLE TO WRITE TO DATABASE");
        }
    }
}

