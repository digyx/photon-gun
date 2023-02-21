use std::{fmt::Debug, process::exit};

use clap::{Parser, Subcommand};
use serde::Deserialize;
use tokio::{
    signal::unix::{signal, SignalKind},
    sync::mpsc,
};
use tonic::{metadata::MetadataValue, transport::Channel};
use tracing_subscriber::prelude::*;

use photon_gun::{healthcheck::HealthcheckService, PhotonGunClient, PingRequest};
use tracing::{error, info};
use uuid::Uuid;

lazy_static::lazy_static! {
    pub static ref AGENT_UUID: Uuid = Uuid::new_v4();
}

#[derive(Debug, Parser)]
struct ClapArgs {
    #[clap(subcommand)]
    action: Action,

    #[clap(long = "server", short = 's')]
    server_endpoint: String,
    #[clap(long = "secret", env = "PHOTON_GUN_SECRET_KEY")]
    secret_key: String,

    /// Logging level (error, warn, info, debug, trace)
    #[clap(long = "log", default_value = "info")]
    logging_level: tracing::Level,
}

#[derive(Debug, Subcommand)]
enum Action {
    Start {
        #[clap(long = "config")]
        config_path: String,
    },
    Ping,
}

#[derive(Deserialize)]
struct Config {
    healthchecks: Vec<Check>,
}

#[derive(Deserialize)]
struct Check {
    endpoint: String,
    interval: i32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = ClapArgs::parse();

    let filter = tracing_subscriber::filter::Targets::new()
        .with_target("photon_gun", args.logging_level)
        .with_target("photon_agent", args.logging_level);

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .init();

    // Initialize client
    let token: MetadataValue<_> = format!("Bearer {}", args.secret_key).parse().unwrap();
    let channel = Channel::from_shared(args.server_endpoint)?
        .connect()
        .await?;
    let mut client =
        PhotonGunClient::with_interceptor(channel, move |mut req: tonic::Request<()>| {
            req.metadata_mut().insert("authorization", token.clone());
            Ok(req)
        });

    match args.action {
        Action::Start { config_path } => {
            // Ping to make sure everything's alright
            info!("pinging server...");
            if let Err(err) = client.ping(PingRequest {}).await {
                println!("error: {}", err.message());
                exit(1)
            };
            info!("pong received.");

            // Read in healthchecks
            let conf = tokio::fs::read_to_string(&config_path).await?;
            let conf: Config = toml::from_str(&conf)?;

            let (sender, mut receiver) = mpsc::unbounded_channel();

            // Listen for checks and send results to server
            info!("starting client listener...");
            let _client_handle = tokio::spawn(async move {
                while let Some(check_result) = receiver.recv().await {
                    let res = client.create_healthcheck(check_result).await;
                    if let Err(err) = res {
                        error!(error = %err);
                        continue;
                    }
                }
            });

            // Spawn healthchecks
            info!("spawning healthchecks...");
            let handles = conf
                .healthchecks
                .into_iter()
                .map(|check| {
                    HealthcheckService::new(
                        check.endpoint,
                        check.interval,
                        &AGENT_UUID,
                        sender.clone(),
                    )
                })
                .map(|check| check.spawn());

            for handle in handles {
                handle.await;
            }

            let mut sigint = signal(SignalKind::interrupt())?;
            let mut sigterm = signal(SignalKind::terminate())?;

            tokio::select! {
                _ = sigint.recv() => info!(msg = "SIGINT received"),
                _ = sigterm.recv() => info!(msg = "SIGTERM received"),
            }

            // TODO: Wait for all handlers to stop
        }

        Action::Ping => match client.ping(PingRequest {}).await {
            Ok(_) => println!("Pong."),
            Err(err) => {
                println!("error: {}", err.message());
                exit(1)
            }
        },
    };

    Ok(())
}
