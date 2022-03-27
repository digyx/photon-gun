use std::time::Duration;

use healthcheck::HealthcheckResult;
use tokio::{time::sleep,signal};
use tracing::{error,info,debug};

mod config;
mod healthcheck;
mod db;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Load YAML config from default directory
    // TODO: Allow user to pass directory via CLI
    // TODO: Imeplement clap for CLI options
    let conf = config::load_config("/etc/photon-gun/conf.yml");

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
    signal::ctrl_c().await.expect("failed to listen for event");
    info!("SIGINT Receieved.");

    info!("Aborting...");
    for handle in handlers {
        handle.abort();
    }

    info!("Tasks stopped.");
}

#[tracing::instrument(skip(service))]
async fn shoot(service: config::BasicCheck) {
    let db_client = match db::DB::new(service.name.clone()).await {
        Ok(client) => client,
        Err(_) => {
            // Postgres error logged in `DB::new` function
            error!(target: "service_events", error = "UNABLE TO CONNECT TO DATABASE");
            return
        }
    };

    info!(target: "service_events", msg = "starting healthchecks");

    // TODO: Determine if check is simple or complex BEFORE starting the loop
    // currently it does neither, so there's no real issue right now
    let http_client = reqwest::Client::new();
    loop {
        let res = match healthcheck::healthcheck(&http_client, &service.endpoint).await {
            HealthcheckResult::Pass => true,
            HealthcheckResult::Fail => false,
            // Reqwest error logged in `healthcheck` function
            HealthcheckResult::Error(_) => {
                error!(target: "service_events",err = "UNABLE TO SEND HTTP REQUEST");
                return
            }
        };

        if db_client.record_healthcheck(res).await.is_err() {
            // Postgres error logged in `record_healthcheck` function
            error!(target: "service_events", err = "UNABLE TO WRITE TO DATABASE");
            return
        }

        debug!(target: "service_events", msg = "sleeping", duration = service.interval);
        sleep(Duration::from_secs(service.interval)).await;
    }
}

