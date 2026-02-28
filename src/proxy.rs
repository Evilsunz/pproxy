use crate::config::RPConfig;
use crate::consul::ConsulDiscovery;
use crate::lb::{AuthVerifier, ConsulNode, ConsulNodes, Context, LoadBalancers, NetIqLoadBalancer};
use crate::{log_error, log_info, log_trace};
use async_trait::async_trait;
use bytes::Bytes;
use dashmap::DashMap;
use pingora::ErrorSource::Upstream;
use pingora::lb::LoadBalancer;
use pingora::prelude::{ProxyHttp, RoundRobin, Session};
use pingora::{Error, HTTPStatus, ImmutStr, RetryType};
use pingora_core::prelude::HttpPeer;
use pingora_core::server::ShutdownWatch;
use pingora_core::services::background::BackgroundService;
use std::sync::Arc;
use tokio::sync::mpsc;

#[async_trait]
impl ProxyHttp for NetIqLoadBalancer {
    type CTX = Context;
    fn new_ctx(&self) -> Self::CTX {
        Context {
            hostname: None,
            fully_qualified_upstream: None,
        }
    }

    async fn request_filter(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> pingora::Result<bool> {
        let hostname = self.get_host(session).ok_or_else(|| {
            Box::new(Error {
                etype: HTTPStatus(503),
                esource: Upstream,
                retry: RetryType::Decided(false),
                cause: None,
                context: Some(ImmutStr::Static("Hostname not resolved")),
            })
        })?;
        //OAUTH2 challenge
        if  self.rp_config.hosts_under_sso.contains(&hostname){
            return self.auth_verifier.verify_auth_cookie(session).await;
        };

        log_trace!("request summary {}", session.request_summary());
        let upstream = self.resolve_upstream(&hostname);
        ctx.hostname = Some(hostname);
        ctx.fully_qualified_upstream = upstream;

        Ok(false)
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
                    log_error!("Failed to send error response: {:?}", e);
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
        let upstream = match self.balancers.get(upstream_name) {
            Some(x) => x,
            None => {
                log_error!("Balancer not found for upstream: {}", upstream_name);
                return Err(Box::new(Error {
                    etype: HTTPStatus(502),
                    esource: Upstream,
                    retry: RetryType::Decided(true),
                    cause: None,
                    context: Option::from(ImmutStr::Owned(format!("Balancer {} not found", upstream_name).into_boxed_str())),
                }));
            },
        }
        .select(b"", 256)
        .unwrap();
        let peer = Box::new(HttpPeer::new(
            upstream,
            false,
            "one.one.one.one".to_string(),
        ));
        Ok(peer)
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
        let rp_config = self.rp_config.clone();
        let (tx, mut rx) = mpsc::channel::<ConsulNodes>(1);
        let handle = tokio::spawn(async move { ConsulDiscovery::new(rp_config).fetch_nodes(tx).await });
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
    pub fn new(rp_config: RPConfig) -> Self {
        //TODO workaround static consul-ui
        let static_consul_ui_ips = vec![rp_config.static_consul_agent_ip_port.clone()];
        let balancer = LoadBalancer::<RoundRobin>::try_from_iter(static_consul_ui_ips).unwrap();
        let balancers = Arc::new(LoadBalancers::new());
        balancers.insert("consul-ui".to_string(), balancer);
        let auth_verifier = AuthVerifier::new(rp_config.clone());
        Self {
            nodes: Arc::new(DashMap::new()),
            balancers,
            auth_verifier,
            rp_config,
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
        let endpoints: Vec<String> = nodes
            .iter()
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
        self.rp_config
            .host_to_upstream
            .iter()
            .find(|(k, _)| hostname.contains(k.as_str()))
            .map(|(_, v)| v.clone())
    }
}
