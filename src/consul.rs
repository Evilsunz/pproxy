use std::collections::HashMap;
use std::time::Duration;
use dashmap::DashMap;
use pingora::prelude::sleep;
use tokio::sync::mpsc::Sender;
use crate::config::PPConfig;
use crate::lb::{ConsulNode, ConsulNodes};

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
        let mut local_cache : HashMapConsulNodes = HashMap::new();
        loop {
            //TODO make it async way
            for service_name in self.pp_config.consul_service_names.clone() {
                let nodes = reqwest::get(format!("http://nest-consul-dev.nest.r53.xcal.tv:8500/v1/catalog/service/{}", service_name))
                    .await.unwrap()
                    .json::<VecConsulNode>()
                    .await.unwrap();
                let cache_entry = local_cache.get(&service_name);
                if cache_entry.is_none() || *cache_entry.unwrap() != nodes {
                    let dash: ConsulNodes = DashMap::from_iter([(service_name.clone(), nodes.clone())]);
                    local_cache.insert(service_name, nodes);
                    let _ = tx.send(dash).await.unwrap();
                }
            }
            sleep(Duration::from_secs(5)).await;
        }
    }
}


