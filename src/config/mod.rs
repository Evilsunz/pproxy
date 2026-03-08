use clap::Parser;
use std::collections::HashMap;
use std::path::PathBuf;
use serde_derive::{Deserialize, Serialize};
use twelf::{Error, Layer, config};

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
    let conf = RPConfig::with_layers(&[
        Layer::Toml(path),
        //Layer::Env(Some(String::from("APP_"))),
    ])?;
    Ok(conf)
}

#[derive(Debug, Clone, Deserialize,Serialize)]
pub struct UpstreamDetails {
    pub upstream: String,

    #[serde(default)]
    pub sso_req: bool,

    #[serde(default)]
    pub redirect_url: String,

    #[serde(default = "default_health_checks")]
    pub health_checks : String,

    #[serde(default)]
    pub is_upstream_static: bool,

    #[serde(default)]
    pub upstream_static_host_port: String,

    #[serde(default)]
    pub weighted: bool,

    #[serde(default)]
    pub check_name: String,
    
    #[serde(default)]
    pub check_condition: String,

    #[serde(default)]
    pub weight_on_true: u16,

    #[serde(default)]
    pub weight_on_false: u16,
}

#[config]
#[derive(Debug, Default, Clone)]
pub struct RPConfig {
    pub port: u64,
    pub tls_port: u64,

    pub consul_url: String,
    pub consul_pool_secs: u64,
    pub consul_leader_pool_secs: u64,

    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub log_path: String,
    pub log_level: String,

    pub vault_address: String,
    pub role_id: String,
    pub secret_id: String,
    pub path_to_cert_secret: String,

    pub tls_enabled: bool,
    pub tls_private_cert: String,
    pub tls_chain_cert: String,
    pub tls_enable_h2: bool,
    
    pub jwt_cert: String,
    pub jwt_private_cert: String,
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
    pub sso_cookie_expire_dayz: u16,

    pub aws_access_key: String,
    pub aws_secret_key: String,
    pub r53_zone_id: String,

    pub host_to_upstream: HashMap<String, UpstreamDetails>,
    pub r53_fqdns: Vec<String>,
}

fn default_health_checks() -> String {
    "passing".to_string()
}
