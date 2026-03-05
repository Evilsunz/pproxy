use crate::config::RPConfig;
use dashmap::DashMap;
use jsonwebtoken::{DecodingKey, EncodingKey, Validation};
use oauth2::basic::{
    BasicRevocationErrorResponse, BasicTokenIntrospectionResponse, BasicTokenResponse,
};
use oauth2::{EndpointNotSet, EndpointSet, StandardRevocableToken};
use pingora::lb::LoadBalancer;
use pingora::prelude::RoundRobin;
use serde_derive::{Serialize};
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::AtomicBool;
use aws_sdk_route53::Client;
use serde::Deserialize;

pub type ConsulNodes = DashMap<String, Vec<ConsulNode>>;
pub type LoadBalancers = DashMap<String, LoadBalancer<RoundRobin>>;

#[derive(Serialize, Debug, Clone, Eq, PartialEq)]
pub struct ConsulNode {
    #[serde(rename = "Node")]
    pub service_name: String,
    #[serde(rename = "Address")]
    pub address: String,
    #[serde(rename = "ServicePort")]
    pub service_port: u16,
}

impl<'de> Deserialize<'de> for ConsulNode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = ConsulEntryRaw::deserialize(deserializer)?;
        let address = if raw.service.address.is_empty() {
            raw.node.address
        } else {
            raw.service.address
        };
        Ok(Self {
            service_name: raw.service.name,
            address,
            service_port: raw.service.port,
        })
    }
}

#[derive(Deserialize)]
struct ConsulEntryRaw {
    #[serde(rename = "Node")]
    node: ConsulNodeRaw,
    #[serde(rename = "Service")]
    service: ConsulServiceRaw,
}

#[derive(Deserialize)]
struct ConsulNodeRaw {
    #[serde(rename = "Address")]
    address: String,
}

#[derive(Deserialize)]
struct ConsulServiceRaw {
    #[serde(rename = "Service")]
    name: String,
    #[serde(rename = "Address")]
    address: String,
    #[serde(rename = "Port")]
    port: u16,
}

pub struct Context {
    pub hostname: Option<String>,
    pub fully_qualified_upstream: Option<String>,
}

#[derive(Clone)]
pub struct NetIqLoadBalancer {
    pub nodes: Arc<ConsulNodes>,
    pub balancers: Arc<LoadBalancers>,
    pub auth_verifier: AuthVerifier,
    pub rp_config: RPConfig,
}

#[derive(Clone)]
pub struct R53 {
    pub rp_config: RPConfig,
    pub runtime_state: RuntimeState,
}

#[derive(Clone)]
pub struct Vault {
    pub rp_config: RPConfig,
}

#[derive(Clone)]
pub struct Web {
    pub rp_config: RPConfig,
    pub nodes: Arc<DashMap<String, Vec<ConsulNode>>>,
    pub runtime_state: RuntimeState,
}

#[derive(Clone)]
pub struct LeaderRoutine {
    pub rp_config: RPConfig,
    pub session_id: Arc<Mutex<String>>,
    pub http_client: reqwest::Client,
    pub runtime_state: RuntimeState,
}

#[derive(Clone)]
pub struct AuthVerifier {
    pub rp_config: RPConfig,
    pub decoding_key: DecodingKey,
    pub encoding_key: EncodingKey,
    pub validation: Validation,
    pub client: oauth2::Client<
        oauth2::basic::BasicErrorResponse,
        BasicTokenResponse,
        BasicTokenIntrospectionResponse,
        StandardRevocableToken,
        BasicRevocationErrorResponse,
        EndpointSet,
        EndpointNotSet,
        EndpointNotSet,
        EndpointNotSet,
        EndpointSet,
    >,
    pub http_client: oauth2::reqwest::Client,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AuthClaims {
    pub sub: String,
    pub tid: String,
    pub exp: u64,
    pub iat: u64,
    pub iss: String,
    pub aud: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthDecision {
    Exchange { code: String },
    RedirectToSso,
    Proceed,
}

#[derive(Clone)]
pub struct RuntimeState {
    pub is_leader: Arc<AtomicBool>,
    pub ip: Arc<Mutex<String>>,
    pub aws_r53_client: Arc<OnceLock<Client>>,
}

impl RuntimeState {
    pub fn new() -> Self {
        Self {
            is_leader: Arc::new(AtomicBool::new(false)),
            ip: Arc::new(Mutex::new(String::new())),
            aws_r53_client: Arc::new(OnceLock::new()),
        }
    }
}
