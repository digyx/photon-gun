use std::fs;

use clap::Parser;
use serde::Deserialize;
use tracing::{debug, error, info};

use crate::healthcheck;

#[derive(Parser, Debug)]
#[clap(author,version,about,long_about = None)]
pub struct CliArgs {
    /// Filepath to Config File
    #[clap(short, long, default_value = "/etc/photon-gun/conf.yml")]
    pub config_path: String,

    /// Logging level (error, warn, info, debug, trace)
    #[clap(short, long, default_value = "info")]
    pub logging_level: tracing::Level,
}

pub fn load_cli_args() -> CliArgs {
    CliArgs::parse()
}

// ==================== Config File ====================
// TODO: Expand config file (not sure what it all needs yet)
#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    pub postgres: PostgresSettings,
    #[serde(default = "no_basic_checks")]
    pub basic_checks: Vec<healthcheck::BasicCheckConfig>,
    #[serde(default = "no_luxurious_checks")]
    pub luxury_checks: Vec<healthcheck::LuxuryCheckConfig>,
}

fn no_basic_checks() -> Vec<healthcheck::BasicCheckConfig> {
    vec![]
}

fn no_luxurious_checks() -> Vec<healthcheck::LuxuryCheckConfig> {
    vec![]
}

#[derive(Debug, Deserialize)]
pub struct PostgresSettings {
    pub uri: String,
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

fn default_max_connections() -> u32 {
    5
}

fn default_min_connections() -> u32 {
    1
}

pub fn load_config_file(path: String) -> ConfigFile {
    debug!(%path);
    let contents = match fs::read_to_string(path) {
        Ok(contents) => {
            info!(msg = "config file loaded to string");
            debug!(%contents);
            contents
        }
        Err(err) => {
            error!(error = %err);
            panic!("{err}")
        }
    };

    match serde_yaml::from_str(contents.as_str()) {
        Ok(res) => {
            info!(msg = "config loaded from yaml string");
            res
        }
        Err(err) => {
            error!(error = %err);
            panic!("{err}")
        }
    }
}
