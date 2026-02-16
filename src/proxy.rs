use std::sync::Arc;
use async_trait::async_trait;
use bytes::Bytes;
use dashmap::DashMap;
use pingora::{Error, HTTPStatus, ImmutStr, RetryType};
use pingora::ErrorSource::Upstream;
use pingora::lb::LoadBalancer;
use pingora::prelude::{ProxyHttp, RoundRobin, Session};
use pingora_core::prelude::HttpPeer;
use pingora_core::server::ShutdownWatch;
use pingora_core::services::background::BackgroundService;
use tokio::sync::mpsc;
use twelf::reexports::log::error;
use crate::config::PPConfig;
use crate::consul::ConsulDiscovery;
use crate::lb::{ConsulNode, ConsulNodes, Context, LoadBalancers, NetIqLoadBalancer};
use crate::log_info;

#[async_trait]
impl ProxyHttp for NetIqLoadBalancer {
    type CTX = Context;
    fn new_ctx(&self) -> Self::CTX {
        Context {
            hostname: None,
            fully_qualified_upstream: None,
        }
    }

    async fn upstream_peer(&self, _session: &mut Session, _ctx: &mut Self::CTX) -> pingora::Result<Box<HttpPeer>> {
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
        let peer = Box::new(HttpPeer::new(upstream, false, "one.one.one.one".to_string()));
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
        Ok(false)
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
impl BackgroundService for NetIqLoadBalancer {
    async fn start(&self, mut shutdown: ShutdownWatch) {
        log_info!("Starting Consul background service");
        let pp_config = self.pp_config.clone();
        let (tx, mut rx) = mpsc::channel::<ConsulNodes>(1);
        let handle = tokio::spawn(async move { ConsulDiscovery::new(pp_config).fetch_nodes(tx).await });
        loop {
            tokio::select! {
                val = rx.recv() => {
                    if let Some(new_node) = val {
                            log_info!(" ++++++++++++ New nodes: {new_node:?}");
                            self.repopulate_nodes(&new_node);
                            self.repopulate_balancers(&new_node)
                    }
                }
                _ = shutdown.changed() => {
                    log_info!("Shutting down (consul background service)...");
                    handle.abort();
                    break;
                }
            }
        }
    }
}

impl NetIqLoadBalancer {

    pub fn new(pp_config: PPConfig) -> Self {
        //TODO workaround
        let static_consul_ui_ips = vec!(pp_config.static_consul_agent_ip_port.clone());
        let balancer = LoadBalancer::<RoundRobin>::try_from_iter(static_consul_ui_ips).unwrap();
        let balancers =Arc::new(LoadBalancers::new());
        balancers.insert("consul-ui".to_string(), balancer);
        Self {
            nodes: Arc::new(DashMap::new()),
            balancers,
            pp_config,
        }
    }


    fn repopulate_nodes(&self, src: &ConsulNodes) {
        for host in src.iter() {
            let host_name = host.key();
            let nodes = host.value().clone();
            self.nodes.insert(host_name.clone(), nodes);
        }
    }

    fn repopulate_balancers(&self, src: &ConsulNodes) {
        for entry in src.iter() {
            if let Some(balancer) = self.create_balancer(entry.value()) {
                self.balancers.insert(entry.key().clone(), balancer);
            }
        }
    }

    fn create_balancer(&self, nodes: &[ConsulNode]) -> Option<LoadBalancer<RoundRobin>> {
        let endpoints: Vec<String> = nodes.iter()
            .map(|cn| format!("{}:{}", cn.address, cn.service_port))
            .collect();
        LoadBalancer::<RoundRobin>::try_from_iter(endpoints).ok()
    }

    fn get_host(&self, session: &mut Session) -> Option<String> {
        session
            .get_header("Host")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.split(':').next())
            .map(|s| s.to_string())
            .or_else(|| session.req_header().uri.host().map(|s| s.to_string()))
    }

    fn resolve_upstream(&self, hostname: &str) -> Option<String> {
        self.pp_config
            .host_to_upstream.iter()
            .find(|(k, _)| hostname.contains(k.as_str()))
            .map(|(_, v)| v.clone())
    }

}
