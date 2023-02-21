use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;
use photon_gun::PhotonGunServer;
use sqlx::postgres::PgPoolOptions;
use tower_http::{auth::RequireAuthorizationLayer, trace::TraceLayer};
use tracing::{error, info, Level};
use tracing_subscriber::{filter, prelude::*};

#[derive(Debug, Parser)]
struct ClapArgs {
    #[clap(long = "secret", env = "PHOTON_GUN_SECRET_KEY")]
    secret_key: String,

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

    if let Err(err) = photon_gun::initialize_tables(&pool_arc).await {
        error!(%err);
        panic!("{}", err);
    }

    // Spin up the gRPC server
    let context = photon_gun::grpc::Server::new(pool_arc.clone());
    let addr = SocketAddr::from(([127, 0, 0, 1], 8000));
    let svc = PhotonGunServer::new(context);
    let server = tonic::transport::Server::builder()
        .layer(TraceLayer::new_for_grpc())
        .layer(RequireAuthorizationLayer::bearer(&cli_args.secret_key))
        .add_service(svc);

    info!(msg = "starting gRPC server...");
    server.serve(addr).await?;
    info!(msg = "server stopped.");

    Ok(())
}
