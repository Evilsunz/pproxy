use crate::config::{RPConfig, UpstreamDetails};
use crate::utils::get_consul_nodes;
use crate::{log_error, log_info};
use dashmap::DashMap;
use pingora::prelude::sleep;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc::Sender;

use crate::structs::{ConsulNode, ConsulNodes};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub type VecConsulNode = Vec<ConsulNode>;
pub type HashMapConsulNodes = HashMap<String, VecConsulNode>;

pub struct ConsulDiscovery {
    rp_config: RPConfig,
}

impl ConsulDiscovery {
    pub fn new(rp_config: RPConfig) -> Self {
        ConsulDiscovery { rp_config }
    }

    pub async fn fetch_nodes(&self, tx: Sender<ConsulNodes>) {
        log_info!("Starting consul discovery...");

        const MAX_CONCURRENCY: usize = 16;

        let mut cache: HashMapConsulNodes = HashMap::new();
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENCY));
        let poll_interval = Duration::from_secs(self.rp_config.consul_pool_secs);

        loop {
            let consul_url = Arc::<str>::from(self.rp_config.consul_url.clone());
            let upsteams: Vec<UpstreamDetails> =
                self.rp_config.host_to_upstream.values().cloned().collect();

            let mut join_set = JoinSet::new();

            for upstream in upsteams {
                let consul_url = Arc::clone(&consul_url);
                let semaphore = Arc::clone(&semaphore);
                let service_name = upstream.upstream.clone();
                let health_checks = upstream.health_checks.clone();
                
                join_set.spawn(async move {
                    let permit = semaphore.acquire_owned().await;
                    if permit.is_err() {
                        return (
                            service_name,
                            Err(anyhow::anyhow!("Semaphore closed while acquiring permit")),
                        );
                    }
                    let _permit = permit.unwrap();

                    let res = get_consul_nodes(consul_url.as_ref(),
                                               service_name.as_str(),
                                               health_checks.as_str()).await;
                    (service_name, res)
                });
            }

            while let Some(joined) = join_set.join_next().await {
                match joined {
                    Ok((service_name, Ok(nodes))) => {
                        let changed = match cache.get(service_name.as_str()) {
                            Some(cached_nodes) => !nodes.is_empty() && cached_nodes != &nodes,
                            None => !nodes.is_empty(),
                        };

                        if changed {
                            let dash: ConsulNodes = DashMap::new();
                            dash.insert(service_name.clone(), nodes.clone());

                            cache.insert(service_name.clone(), nodes);
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

            sleep(poll_interval).await;
        }
    }
}
