use std::collections::HashMap;
use std::error::Error;
use std::i32::MAX;
use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;
use sqlx::postgres::PgPoolOptions;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Mutex;
use tracing::{error, info, Level};
use tracing_subscriber::{filter, prelude::*};

use photon_gun::healthcheck::HealthcheckService;

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

    /// Enable embedded rest-api webserver
    #[clap(long = "rest-api")]
    enable_rest_api: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli_args = ClapArgs::parse();

    let filter = filter::Targets::new()
        .with_target("photon_gun", cli_args.logging_level)
        .with_target("photon_server", cli_args.logging_level)
        // gRPC access logs
        .with_target("tower_http", cli_args.logging_level)
        // sqlx::query logs ALL sql queries by defualt to Level::INFO
        // This is excessive, so we only allow WARN and ERROR logs from sqlx
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
    let pool_arc = Arc::new(pool);

    if let Err(err) = photon_gun::db::initialize_tables(&pool_arc).await {
        error!(%err);
        panic!("{}", err);
    }

    // Spin off healthchecks into their own Tokio task
    // We save the handlers for aborting later if they are removed or to stop the service
    let mut healthcheck_handlers = HashMap::new();
    let healthcheck_vec = photon_gun::db::list_healthchecks(&pool_arc, true, MAX)
        .await?
        .healthchecks;

    for healthcheck in healthcheck_vec {
        let id = healthcheck.id; // Copy value for later use
        let service = HealthcheckService::new(healthcheck, pool_arc.clone());
        let handle = service.spawn().await;

        healthcheck_handlers.insert(id, handle);
    }

    // Spin up the gRPC server
    info!(msg = "starting gRPC server...");
    let service = photon_gun::grpc::Server::new(pool_arc.clone(), Mutex::new(healthcheck_handlers));
    let addr = SocketAddr::from(([127, 0, 0, 1], 8000));
    let grpc_server = tonic::transport::Server::builder()
        .layer(tower_http::trace::TraceLayer::new_for_grpc())
        .add_service(photon_gun::PhotonGunServer::new(service));
    tokio::task::spawn(grpc_server.serve(addr));
    info!(msg = "server started.", %addr);

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    // Wait for all handlers
    tokio::select! {
        _ = sigint.recv() => info!(msg = "SIGINT received"),
        _ = sigterm.recv() => info!(msg = "SIGTERM received"),
    }

    info!(msg = "aborting tasks...");
    // TODO: Gracefully abort tasks...

    info!(msg = "tasks stopped.");
    Ok(())
}
