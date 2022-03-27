use std::fs;

use clap::Parser;
use serde::Deserialize;
use tracing::{error,info};

#[derive(Parser,Debug)]
#[clap(author,version,about,long_about = None)]
pub struct CliArgs {
    /// Filepath to Config File
    #[clap(short,long, default_value = "/etc/photon-gun/conf.yml")]
    pub config_path: String,

    /// Logging level (error, warn, info, debug, trace)
    #[clap(short, long, default_value = "warn")]
    pub logging_level: tracing::Level,
}

pub fn load_cli_args() -> CliArgs {
    CliArgs::parse()
}

// ==================== Config File ====================
// TODO: Expand config file (not sure what it all needs yet)
#[derive(Debug,Deserialize)]
pub struct ConfigFile {
    pub postgres_uri: String,
    pub basic_checks: Vec<BasicCheck>,
}

#[derive(Debug,Deserialize)]
pub struct BasicCheck{
    pub name: String,
    pub endpoint: String,
    pub interval: u64,
}

#[tracing::instrument]
pub fn load_config_file(path: String) -> ConfigFile {
    let contents = match fs::read_to_string(path) {
        Ok(contents) =>{
            info!(target: "config", msg = "config file loaded to string");
            contents
        },
        Err(err) => {
            error!(target: "config", err = format!("{err}").as_str());
            panic!("{err}")
        },
    };

    match serde_yaml::from_str(contents.as_str()) {
        Ok(res) => {
            info!(target: "config", msg = "config loaded from yaml string");
            res
        },
        Err(err) => {
            error!(target: "config", err = format!("{err}").as_str());
            panic!("{err}")
        },
    }
}

