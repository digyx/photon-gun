use std::fs;

use clap::Parser;
use serde::Deserialize;
use tracing::{debug, error, info};

#[derive(Parser, Debug)]
#[clap(author,version,about,long_about = None)]
pub struct CliArgs {
    /// Filepath to Config File
    #[clap(long = "conf", default_value = "/etc/photon-gun/conf.yml")]
    pub config_path: String,

    /// Filepath to the directory scripts with relative paths use (does not end in "/")
    #[clap(long = "scripts", default_value = "/etc/photon-gun/scripts")]
    pub script_dir: String,

    /// Logging level (error, warn, info, debug, trace)
    #[clap(long = "log", default_value = "info")]
    pub logging_level: tracing::Level,

    /// Enable embedded webserver for clients to request JSON repsentations of the healthchecks
    #[clap(short = 's', long = "server")]
    pub enable_webserver: bool,
}

pub fn load_cli_args() -> CliArgs {
    CliArgs::parse()
}

// ==================== Config File ====================
// TODO: Expand config file (not sure what it all needs yet)
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct ConfigFile {
    pub postgres: PostgresSettings,
    #[serde(default = "no_basic_checks")]
    pub basic_checks: Vec<BasicCheckConfig>,
    #[serde(default = "no_luxurious_checks")]
    pub luxury_checks: Vec<LuxuryCheckConfig>,
}

fn no_basic_checks() -> Vec<BasicCheckConfig> {
    vec![]
}

fn no_luxurious_checks() -> Vec<LuxuryCheckConfig> {
    vec![]
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct BasicCheckConfig {
    pub name: String,
    // HTTP URL the check will send a GET request to
    pub endpoint: String,
    // Length of time in seconds between checks starting
    pub interval: u64,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct LuxuryCheckConfig {
    pub name: String,
    // Path to Lua script to be ran for the check
    // Relative paths start in CONFIG_DIR/scripts/
    #[serde(alias = "script")]
    pub script_path: String,
    // Length of time in seconds between checks starting
    pub interval: u64,
}

pub fn load_config_file(path: &str) -> ConfigFile {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // This exists to check that none of the example configs ever get stale
    fn check_example_configs() {
        let expected = ConfigFile {
            postgres: PostgresSettings {
                uri: "postgres://postgres:password@localhost:5432/photon-gun".into(),
                min_connections: 1,
                max_connections: 5,
            },
            basic_checks: vec![
                BasicCheckConfig {
                    name: "google".into(),
                    endpoint: "https://google.com".into(),
                    interval: 1,
                },
                BasicCheckConfig {
                    name: "vorona".into(),
                    endpoint: "https://vorona.gg/healthcheck".into(),
                    interval: 1,
                },
            ],
            luxury_checks: vec![
                LuxuryCheckConfig {
                    name: "test".into(),
                    script_path: "test.lua".into(),
                    interval: 5,
                },
                LuxuryCheckConfig {
                    name: "random".into(),
                    script_path: "random.lua".into(),
                    interval: 1,
                },
            ],
        };
        let conf = load_config_file("example/config.yml");

        assert_eq!(expected, conf);
    }
}
