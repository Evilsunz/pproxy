use crate::consul::ConsulDiscovery;
use async_trait::async_trait;
use dashmap::DashMap;
use pingora::http::RequestHeader;
use pingora::lb::LoadBalancer;
use pingora::prelude::{HttpPeer, ProxyHttp, RoundRobin, Session};
use pingora::server::ShutdownWatch;
use pingora::services::background::BackgroundService;
use serde_derive::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::config::PPConfig;

pub type ConsulNodes = DashMap<String, Vec<ConsulNode>>;

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
        println!(" +++++++++++++++ Nodes = {:?}" , self.nodes);
        //TODO ROUND_ROBIN NOT WORKING
        let res = self.nodes.get("pipeline-device-portal-rest-api").unwrap()
            .iter().map(|cn| format!("{}:{}",cn.address,cn.service_port)).collect::<Vec<String>>();
        let upstream = LoadBalancer::<RoundRobin>::try_from_iter(res)
        .unwrap()
        .select(b"", 256)
        .unwrap();
        //TODO ROUND_ROBIN NOT WORKING
        println!("upstream peer is: {upstream:?}");
        let peer = Box::new(HttpPeer::new(upstream, false, "one.one.one.one".to_string(),));
        Ok(peer)
    }

    async fn request_filter(&self, _session: &mut Session, _ctx: &mut Self::CTX) -> pingora::Result<bool> {
        // Parse host here -> add upstream to CTX
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
        let _ = tokio::spawn(async move { ConsulDiscovery::new(pp_config).fetch_nodes2(tx).await });
        loop {
            tokio::select! {
                val = rx.recv() => {
                    match val {
                        Some(new_nodes) => {
                            println!(" ++++++++++++ New nodes: {new_nodes:?}");
                            clone_dashmap(&new_nodes, &self.nodes);
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

pub fn clone_dashmap(src: &ConsulNodes, dst: &ConsulNodes) {
    for host in src.iter(){
        let host_name = host.key();
        let nodes = host.value().clone();
        dst.insert(host_name.clone(), nodes);
    }
}
