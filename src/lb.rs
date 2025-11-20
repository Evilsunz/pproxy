use std::collections::BTreeSet;
use crate::consul::ConsulDiscovery;
use async_trait::async_trait;
use dashmap::DashMap;
use pingora::http::RequestHeader;
use pingora::lb::{LoadBalancer};
use pingora::prelude::{HttpPeer, ProxyHttp, RoundRobin, Session};
use pingora::server::ShutdownWatch;
use pingora::services::background::BackgroundService;
use serde_derive::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
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

#[derive(Clone)]
pub struct LB {
    pub nodes: Arc<ConsulNodes>,
    pub balancers: Arc<LoadBalancers>,
    pub pp_config: PPConfig,
}

#[async_trait]
impl ProxyHttp for LB {
    type CTX = ();
    fn new_ctx(&self) -> () {
        ()
    }

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        _ctx: &mut (),
    ) -> pingora::Result<Box<HttpPeer>> {
        let upstream = self.balancers.get("pipeline-device-portal-rest-api")
        .unwrap()
        .select(b"", 256)
        .unwrap();
        println!("upstream peer is: {upstream:?}");
        let peer = Box::new(HttpPeer::new(upstream, false, "one.one.one.one".to_string(),));
        Ok(peer)
    }

    async fn request_filter(&self, _session: &mut Session, _ctx: &mut Self::CTX) -> pingora::Result<bool> {
        println!(" ++++++ host {:?}", Self::get_host(_session));
        Ok(false)
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> pingora::Result<()> {
        upstream_request
            .insert_header("Host", "one.one.one.one")
            .unwrap();
        Ok(())
    }
}

#[async_trait]
impl BackgroundService for LB {
    async fn start(&self, mut shutdown: ShutdownWatch) {
        println!("Starting Consul background service");
        let pp_config = self.pp_config.clone();
        let (tx,mut rx) = mpsc::channel::<ConsulNodes>(1);
        let _ = tokio::spawn(async move { ConsulDiscovery::new(pp_config).fetch_nodes(tx).await });
        loop {
            tokio::select! {
                val = rx.recv() => {
                    match val {
                        Some(new_nodes) => {
                            println!(" ++++++++++++ New nodes: {new_nodes:?}");
                            self.clone_dashmap(&new_nodes, &self.nodes);
                            self.populate_balancers()
                        }
                        None => {
                        }
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
        for host in src.iter(){
            let host_name = host.key();
            let nodes = host.value().clone();
            dst.insert(host_name.clone(), nodes);
        }
    }

    //TODO - refresh only changed nodes
    fn populate_balancers(&self){
        for host in self.nodes.iter() {
            let host_name = host.key();
            let nodes = host.value().clone().iter()
                .map(|cn| format!("{}:{}",cn.address,cn.service_port)).collect::<Vec<String>>();
            let upstreamz = LoadBalancer::<RoundRobin>::try_from_iter(nodes)
                .unwrap();
            self.balancers.insert(host_name.clone(), upstreamz);
        }
    }

    fn get_host(session: &mut Session) -> Option<String> {
        if let Some(host) = session.get_header("Host") {
            let host_port = host.to_str().expect("Expecting host name in request").splitn(2, ':').collect::<Vec<&str>>();
            return Some(host_port[0].to_string());
        }

        if let Some(host) = session.req_header().uri.host() {
            return Some(host.to_string());
        }
        None
    }

}
