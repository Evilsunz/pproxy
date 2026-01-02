use std::collections::HashMap;
use clap::Parser;
use serde::{Serialize};
use std::path::PathBuf;
use twelf::{config, Layer, Error};
use crate::utils::resolve_ip;

#[derive(Parser, Debug)]
#[command(version,long_about = None, ignore_errors=true)]
pub struct Args {
    #[arg(short, long, default_value_t = String::from("./config/conf.toml"), env("APP_CONFIG_PATH"))]
    pub config_path: String,
}

pub fn parse() -> Args {
    Args::parse()
}

pub fn load(path: PathBuf) -> Result<PPConfig, Error> {
    let mut conf = PPConfig::with_layers(&[
        Layer::Toml(path),
        //Layer::Env(Some(String::from("APP_"))),
    ])?;
    let ip = resolve_ip().unwrap_or_else(|_| panic!("Unable to resolve own IP - shutting down..."));
    conf.ip = Some(ip);
    Ok(conf)
}

#[config]
#[derive(Debug, Default, Serialize, Clone)]
pub struct PPConfig {
    pub port : u64,
    pub tls_port : u64,

    pub consul_url : String,
    pub consul_pool_secs : u64,

    pub vault_address : String,
    pub role_id : String,
    pub secret_id : String,
    pub path_to_cert_secret : String,

    pub tls_enabled : bool,
    pub tls_private_cert : String,
    pub tls_chain_cert : String,

    pub aws_access_key : String,
    pub aws_secret_key : String,
    pub r53_zone_id : String,

    pub host_to_upstream : HashMap<String, String>,
    pub fqdns : Vec<String>,

    pub ip : Option<String>,
    pub is_leader : Option<bool>,
}