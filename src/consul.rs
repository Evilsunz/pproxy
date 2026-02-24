use std::collections::HashMap;
use std::time::Duration;
use dashmap::DashMap;
use pingora::prelude::sleep;
use tokio::sync::mpsc::Sender;
use crate::config::RPConfig;
use crate::lb::{ConsulNode, ConsulNodes};
use crate::{log_error, log_info};
use crate::utils::get_consul_nodes;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use std::sync::Arc;

pub type VecConsulNode = Vec<ConsulNode>;
pub type HashMapConsulNodes = HashMap<String, VecConsulNode>;

pub struct ConsulDiscovery{
    rp_config: RPConfig
}

impl ConsulDiscovery {

    pub fn new(rp_config: RPConfig) -> Self {
        ConsulDiscovery{
            rp_config
        }
    }

    pub async fn fetch_nodes(&self, tx: Sender<ConsulNodes>) {
        log_info!("Starting consul discovery...");
        let mut local_cache: HashMapConsulNodes = HashMap::new();
        let sem = Arc::new(Semaphore::new(16));

        loop {
            let consul_url = self.rp_config.consul_url.clone();
            let service_names: Vec<String> = self
                .rp_config
                .host_to_upstream
                .values()
                .cloned()
                .collect();

            let mut join_set = JoinSet::new();

            for service_name in service_names {
                let consul_url = consul_url.clone();
                let sem = Arc::clone(&sem);

                join_set.spawn(async move {
                    let _permit = sem.acquire_owned().await;
                    let res = get_consul_nodes(consul_url.as_str(), service_name.as_str()).await;
                    (service_name, res)
                });
            }

            while let Some(joined) = join_set.join_next().await {
                match joined {
                    Ok((service_name, Ok(nodes))) => {
                        let changed = local_cache
                            .get(service_name.as_str())
                            .map_or(true, |cached| cached != &nodes);

                        if changed {
                            let dash: ConsulNodes =
                                DashMap::from_iter([(service_name.clone(), nodes.clone())]);
                            local_cache.insert(service_name.clone(), nodes);
                            let _ = tx.send(dash).await;
                        }
                    }
                    Ok((service_name, Err(err))) => {
                        log_error!(
                            "Error happened during consul nodes serde (proceeding) for {}: {}",
                            service_name,
                            err
                        );
                    }
                    Err(join_err) => {
                        log_error!("Consul discovery task failed: {}", join_err);
                    }
                }
            }

            sleep(Duration::from_secs(self.rp_config.consul_pool_secs)).await;
        }
    }
}


