use std::collections::HashMap;
use clap::Parser;
use std::path::PathBuf;
use aws_sdk_route53::Client;
use twelf::{config, Layer, Error};
use crate::utils::{aws_r53_client, resolve_ip};

#[derive(Parser, Debug)]
#[command(version,long_about = None, ignore_errors=true)]
pub struct Args {
    #[arg(
      short = 't',
      long = "rproxy-config",
      default_value_t = String::from("/opt/rproxy/config/rproxy.toml"),
      env("APP_CONFIG_PATH")
    )]
    pub config_path: String,
}

pub fn parse() -> Args {
    Args::parse()
}

pub fn load(path: PathBuf) -> Result<RPConfig, Error> {
    let mut conf = RPConfig::with_layers(&[
        Layer::Toml(path),
        //Layer::Env(Some(String::from("APP_"))),
    ])?;
    let ip = resolve_ip().unwrap_or_else(|_| panic!("Unable to resolve own IP - shutting down..."));
    let aws_r53_client = aws_r53_client(conf.aws_access_key.clone(), conf.aws_secret_key.clone());
    conf.ip = Some(ip);
    conf.aws_r53_client = Some(aws_r53_client);
    Ok(conf)
}

#[config]
#[derive(Debug, Default, Clone)]
pub struct RPConfig {
    pub port : u64,
    pub tls_port : u64,

    pub consul_url : String,
    pub consul_pool_secs : u64,
    pub consul_leader_pool_secs : u64,

    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub log_path: String,
    pub log_level: String,
    pub log_groups: Vec<String>,
    
    pub static_consul_agent_ip_port: String,
    
    pub vault_address : String,
    pub role_id : String,
    pub secret_id : String,
    pub path_to_cert_secret : String,

    pub tls_enabled : bool,
    pub tls_private_cert : String,
    pub tls_chain_cert : String,

    pub jwt_cert: String,
    pub jwt_private_cert: String,
    pub hosts_under_sso: Vec<String>,
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub redirect_url: String,
    pub scopes: Vec<String>,
    
    
    pub aws_access_key : String,
    pub aws_secret_key : String,
    pub r53_zone_id : String,

    pub host_to_upstream : HashMap<String, String>,
    pub fqdns : Vec<String>,

    #[serde(skip, default)]
    pub ip : Option<String>,
    #[serde(skip, default)]
    pub aws_r53_client : Option<Client>,
    #[serde(skip, default)]
    pub is_leader : Option<bool>,
}