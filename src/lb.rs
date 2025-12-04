use async_trait::async_trait;
use bytes::Bytes;
use dashmap::DashMap;
use pingora::ErrorSource::Upstream;
use pingora::lb::LoadBalancer;
use pingora::prelude::{HttpPeer, ProxyHttp, RoundRobin, Session};
use pingora::server::ShutdownWatch;
use pingora::services::background::BackgroundService;
use pingora::{Error, HTTPStatus, ImmutStr, RetryType};
use serde_derive::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use twelf::reexports::log::error;
use crate::config::PPConfig;

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
pub struct LB {
    pub nodes: Arc<ConsulNodes>,
    pub balancers: Arc<LoadBalancers>,
    pub pp_config: PPConfig,
}

#[derive(Clone)]
pub struct R53 {
    pub pp_config: PPConfig,
}

#[derive(Clone)]
pub struct Vault {
    pub pp_config: PPConfig,
}