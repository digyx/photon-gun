use std::fs;
use std::{error::Error, sync::Arc, time::Duration};

use sqlx::postgres::PgPoolOptions;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{debug, info, Level};
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
    let pool = PgPoolOptions::new()
        .min_connections(conf.postgres.min_connections)
        .max_connections(conf.postgres.max_connections)
        .connect(&conf.postgres.uri)
        .await?;
    let pool_arc = Arc::new(pool);

    // Spin off basic check off into its own Tokio task
    // We save the handlers for aborting later, if necessary
    let mut handlers = vec![];
    for service in conf.basic_checks {
        info!(%service.name, msg = "starting basic check...");

        // Increment the RC on the PgPool ARC to deal with the move
        let db_client = pool_arc.clone();
        // Create the database table for the basic check
        // Every basic check gets its own table
        db::create_healthcheck_table(&db_client, &service.name).await?;

        // Ensures that the tasks runs every two seconds without being affected by the execution
        // time.  This does mean checks can overlap if execution takes too long
        let mut interval = tokio::time::interval(Duration::from_secs(service.interval));

        let basic_check = healthcheck::BasicCheck::new(service, db_client);

        let task = tokio::task::spawn(async move {
            debug!(?basic_check);

            loop {
                // Tik tok
                // Initial call is passed through immediately
                interval.tick().await;
                basic_check.spawn().await;
            }
        });

        handlers.push(task);
    }

    for service in conf.luxury_checks {
        info!(%service.name, msg = "starting luxury check...");

        let db_client = pool_arc.clone();
        db::create_healthcheck_table(&db_client, &service.name).await?;

        let mut interval = tokio::time::interval(Duration::from_secs(service.interval));

        let lua_script = fs::read_to_string(&format!("example/scripts/{}", service.script_path))?;
        let luxury_check_arc = Arc::new(healthcheck::LuxuryCheck::new(
            service, db_client, lua_script,
        ));

        let task = tokio::task::spawn(async move {
            debug!(?luxury_check_arc);

            loop {
                interval.tick().await;
                let luxury_check = luxury_check_arc.clone();

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
