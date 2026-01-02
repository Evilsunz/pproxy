use std::collections::HashMap;
use std::time::Duration;
use dashmap::DashMap;
use pingora::prelude::sleep;
use tokio::sync::mpsc::Sender;
use crate::config::PPConfig;
use crate::lb::{ConsulNode, ConsulNodes};
use crate::utils::get_consul_nodes;

pub type VecConsulNode = Vec<ConsulNode>;
pub type HashMapConsulNodes = HashMap<String, VecConsulNode>;

pub struct ConsulDiscovery{
    pp_config: PPConfig
}

impl ConsulDiscovery {

    pub fn new(pp_config: PPConfig) -> Self {
        ConsulDiscovery{
            pp_config
        }
    }

    pub async fn fetch_nodes(&self, tx: Sender<ConsulNodes>) {
        println!("Starting consul discovery...");
        let mut local_cache: HashMapConsulNodes = HashMap::new();
        loop {
            //TODO make it async way
            for service_name in self.pp_config.host_to_upstream.values().clone() {
                let nodes = match get_consul_nodes(self.pp_config.consul_url.as_str(), service_name).await {
                    Ok(nodes) => nodes,
                    Err(err) => {
                        println!("Error happened during consul nodes serde (proceeding) : {}" , err);
                        continue;
                    }
                };
                let cache_entry = local_cache.get(service_name);
                if cache_entry.is_none() || *cache_entry.unwrap() != nodes {
                    let dash: ConsulNodes = DashMap::from_iter([(service_name.clone(), nodes.clone())]);
                    local_cache.insert(service_name.to_string(), nodes);
                    let _ = tx.send(dash).await;
                }
            }
            sleep(Duration::from_secs(self.pp_config.consul_pool_secs)).await;
        }
    }
}


