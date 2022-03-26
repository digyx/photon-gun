use std::fs;

use serde::Deserialize;
use tracing::{error,info};

// TODO: Expand config file (not sure what it all needs yet)
pub type Config = Vec<ServiceConfig>;

#[derive(Debug,Deserialize)]
pub struct ServiceConfig {
    pub name: String,
    pub endpoint: String,
    pub interval: u64,
    pub validate: bool,
    pub lua_script: String,
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

