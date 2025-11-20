use std::collections::HashMap;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use twelf::{config, Layer, Error};

#[derive(Parser, Debug)]
#[command(version)]
pub struct Args {
    #[arg(short, long, default_value_t = String::from("./config/conf.toml"), env("APP_CONFIG_PATH"))]
    pub config_path: String,
}

pub fn parse() -> Args {
    Args::parse()
}

pub fn load(path: PathBuf) -> Result<PPConfig, Error> {
    let path = path.into();
    let conf = PPConfig::with_layers(&[
        Layer::Toml(path),
        //Layer::Env(Some(String::from("APP_"))),
    ])?;
    Ok(conf)
}
#[config]
#[derive(Debug, Default, Serialize, Clone)]
pub struct PPConfig {
    pub port : u32,
    pub host_to_upstream : HashMap<String, String>,
}