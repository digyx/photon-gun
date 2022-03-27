use std::fs;

use serde::Deserialize;
use tracing::{error,info};

// TODO: Expand config file (not sure what it all needs yet)
#[derive(Debug,Deserialize)]
pub struct Config {
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
pub fn load_config(path: &str) -> Config {
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

