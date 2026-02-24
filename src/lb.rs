use dashmap::DashMap;
use pingora::lb::LoadBalancer;
use pingora::prelude::{RoundRobin};
use serde_derive::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use crate::config::RPConfig;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Validation};

pub type ConsulNodes = DashMap<String, Vec<ConsulNode>>;
pub type LoadBalancers = DashMap<String, LoadBalancer<RoundRobin>>;

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
pub struct ConsulNode {
    #[serde(rename = "Node")]
    pub node: String,
    #[serde(rename = "Address")]
    pub address: String,
    #[serde(rename = "ServicePort")]
    pub service_port: u64,
}

pub struct Context {
    pub hostname: Option<String>,
    pub fully_qualified_upstream: Option<String>,
}

#[derive(Clone)]
pub struct NetIqLoadBalancer {
    pub nodes: Arc<ConsulNodes>,
    pub balancers: Arc<LoadBalancers>,
    pub rp_config: RPConfig,
}

#[derive(Clone)]
pub struct R53 {
    pub rp_config: RPConfig,
}

#[derive(Clone)]
pub struct Vault {
    pub rp_config: RPConfig,
}

#[derive(Clone)]
pub struct Web {
    pub rp_config: RPConfig,
    pub nodes: Arc<DashMap<String, Vec<ConsulNode>>>,
}

#[derive(Clone)]
pub struct LeaderRoutine{
    pub rp_config: RPConfig,
    pub session_id: Arc<Mutex<String>>,
}

pub struct AuthVerifier {
    pub decoding_key: DecodingKey,
    pub encoding_key: EncodingKey,
    pub validation: Validation,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AuthClaims {
    pub sub: String,
    pub tid: String,
    pub exp: i64,
    pub iat: i64,
    pub iss: String,
    pub aud: String,
}