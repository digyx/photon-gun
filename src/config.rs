use std::collections::HashMap;
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
    #[serde(default = "no_luxury_checks")]
    pub luxury_checks: Vec<LuxuryCheckConfig>,
}

fn no_basic_checks() -> Vec<BasicCheckConfig> {
    vec![]
}

fn no_luxury_checks() -> Vec<LuxuryCheckConfig> {
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
    pub id: i32,
    pub name: String,
    // HTTP URL the check will send a GET request to
    pub endpoint: String,
    // Length of time in seconds between checks starting
    pub interval: u64,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct LuxuryCheckConfig {
    pub id: i32,
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

    let conf: ConfigFile = match serde_yaml::from_str(contents.as_str()) {
        Ok(res) => {
            info!(msg = "config loaded from yaml string");
            res
        }
        Err(err) => {
            error!(error = %err);
            panic!("{err}")
        }
    };

    // This next bit is to ensure that there are no duplicate service names
    //
    // Combine all service names into one iterator
    let service_name_vec = conf
        .basic_checks
        .iter()
        .map(|check| check.name.as_str())
        .chain(conf.luxury_checks.iter().map(|check| check.name.as_str()));

    // cmp is used for storing already seen service names
    let mut cmp: HashMap<&str, bool> = HashMap::new();
    let mut duplicate_service_names = false;

    // Insert name into a hashmap and if an update occurs (returning Some) then mark that there are
    // duplicates and log the error to console
    service_name_vec.for_each(|name| {
        if cmp.insert(name, true).is_some() {
            error!("duplicate service name: {}", name);
            duplicate_service_names = true;
        }
    });

    if duplicate_service_names {
        panic!("invalid config: duplicate service names");
    }

    conf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // These exist to check that none of the example configs ever get stale
    fn check_example_conf_yml() {
        let expected = ConfigFile {
            postgres: PostgresSettings {
                uri: "postgres://postgres:password@localhost:5432/photon-gun".into(),
                min_connections: 1,
                max_connections: 5,
            },
            basic_checks: vec![
                BasicCheckConfig {
                    id: 0,
                    name: "google".into(),
                    endpoint: "https://google.com".into(),
                    interval: 1,
                },
                BasicCheckConfig {
                    id: 0,
                    name: "vorona".into(),
                    endpoint: "https://vorona.gg/healthcheck".into(),
                    interval: 1,
                },
            ],
            luxury_checks: vec![
                LuxuryCheckConfig {
                    id: 0,
                    name: "test".into(),
                    script_path: "test.lua".into(),
                    interval: 5,
                },
                LuxuryCheckConfig {
                    id: 0,
                    name: "random".into(),
                    script_path: "random.lua".into(),
                    interval: 1,
                },
            ],
        };
        let conf = load_config_file("example/config.yml");

        assert_eq!(expected, conf);
    }

    #[test]
    #[should_panic(expected = "invalid config: duplicate service names")]
    fn check_example_fail_duplicate_names_yml() {
        load_config_file("example/test_duplicate_names.yml");
    }

    #[test]
    // These should absolutely never fail
    fn default_value_tests() {
        assert_eq!(5, default_max_connections());
        assert_eq!(1, default_min_connections());
        assert!(no_basic_checks().is_empty());
        assert!(no_luxury_checks().is_empty());
    }
}
