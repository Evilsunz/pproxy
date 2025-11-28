use crate::config::PPConfig;
use crate::consul::ConsulDiscovery;
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

#[derive(Clone)]
pub struct LB {
    pub nodes: Arc<ConsulNodes>,
    pub balancers: Arc<LoadBalancers>,
    pub pp_config: PPConfig,
}

pub struct Context {
    hostname: Option<String>,
    fully_qualified_upstream: Option<String>,
}

#[async_trait]
impl ProxyHttp for LB {
    type CTX = Context;
    fn new_ctx(&self) -> Self::CTX {
        Context {
            hostname: None,
            fully_qualified_upstream: None,
        }
    }

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> pingora::Result<Box<HttpPeer>> {
        let upstream_name = match _ctx.fully_qualified_upstream.as_ref() {
            Some(x) => x,
            None => {
                if let Err(e) = _session
                    .respond_error_with_body(502, Bytes::from("502 Bad Gateway\n"))
                    .await
                {
                    error!("Failed to send error response: {:?}", e);
                }
                return Err(Box::new(Error {
                    etype: HTTPStatus(502),
                    esource: Upstream,
                    retry: RetryType::Decided(false),
                    cause: None,
                    context: Option::from(ImmutStr::Static("Upstream not found")),
                }));
            }
        };
        let upstream = self
            .balancers
            .get(upstream_name)
            .unwrap()
            .select(b"", 256)
            .unwrap();
        // println!("upstream peer is: {upstream:?}");
        let peer = Box::new(HttpPeer::new(
            upstream,
            false,
            "one.one.one.one".to_string(),
        ));
        Ok(peer)
    }

    async fn request_filter(
        &self,
        _session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> pingora::Result<bool> {
        if let Some(hostname) = self.get_host(_session) {
            let upstream = self.resolve_upstream(&hostname);
            _ctx.hostname = Some(hostname);
            _ctx.fully_qualified_upstream = upstream;
        };
        Ok(true)
    }

    // async fn upstream_request_filter(
    //     &self,
    //     _session: &mut Session,
    //     upstream_request: &mut RequestHeader,
    //     _ctx: &mut Self::CTX,
    // ) -> pingora::Result<()> {
    //     upstream_request
    //         .insert_header("Host", "one.one.one.one")
    //         .unwrap();
    //     Ok(())
    // }
}

#[async_trait]
impl BackgroundService for LB {
    async fn start(&self, mut shutdown: ShutdownWatch) {
        println!("Starting Consul background service");
        let pp_config = self.pp_config.clone();
        let (tx, mut rx) = mpsc::channel::<ConsulNodes>(1);
        tokio::spawn(async move { ConsulDiscovery::new(pp_config).fetch_nodes(tx).await });
        loop {
            tokio::select! {
                val = rx.recv() => {
                    if let Some(new_node) = val {
                            println!(" ++++++++++++ New nodes: {new_node:?}");
                            self.clone_dashmap(&new_node, &self.nodes);
                            self.populate_balancers()
                    }
                }
                _ = shutdown.changed() => {
                    break;
                }
            }
        }
    }
}

impl LB {
    fn clone_dashmap(&self, src: &ConsulNodes, dst: &ConsulNodes) {
        for host in src.iter() {
            let host_name = host.key();
            let nodes = host.value().clone();
            dst.insert(host_name.clone(), nodes);
        }
    }

    //TODO - refresh only changed nodes
    fn populate_balancers(&self) {
        for host in self.nodes.iter() {
            let host_name = host.key();
            let nodes = host
                .value()
                .clone()
                .iter()
                .map(|cn| format!("{}:{}", cn.address, cn.service_port))
                .collect::<Vec<String>>();
            let upstreamz = LoadBalancer::<RoundRobin>::try_from_iter(nodes).unwrap();
            self.balancers.insert(host_name.clone(), upstreamz);
        }
    }

    fn get_host(&self, session: &mut Session) -> Option<String> {
        if let Some(host) = session.get_header("Host") {
            let host_port = host
                .to_str()
                .expect("Expecting host name in request")
                .splitn(2, ':')
                .collect::<Vec<&str>>();
            return Some(host_port[0].to_string());
        }

        if let Some(host) = session.req_header().uri.host() {
            return Some(host.to_string());
        }
        None
    }

    fn resolve_upstream(&self, hostname: &str) -> Option<String> {
        for (k, v) in self.pp_config.host_to_upstream.iter() {
            if hostname.contains(k) {
                return Some(v.clone());
            }
        }
        None
    }
}
