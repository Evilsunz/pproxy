use std::collections::HashMap;
use std::time::Duration;
use dashmap::DashMap;
use pingora::prelude::sleep;
use tokio::sync::mpsc::Sender;
use crate::lb::{ConsulNode, ConsulNodes};

pub type VecConsulNode = Vec<ConsulNode>;
pub type HashMapConsulNodes = HashMap<String, VecConsulNode>;

pub struct ConsulDiscovery{
}

impl ConsulDiscovery {

    pub fn new() -> Self {
        ConsulDiscovery{}
    }

    pub async fn fetch_nodes(&self, tx: Sender<ConsulNodes>) {
        println!("Starting consul discovery...");
        let mut local_cache : HashMapConsulNodes = HashMap::new();
        loop {
            let mut result : HashMapConsulNodes = HashMap::new();
            let nodes = reqwest::get("http://nest-consul-dev.nest.r53.xcal.tv:8500/v1/catalog/service/pipeline-device-portal-rest-api")
                .await.unwrap()
                .json::<VecConsulNode>()
                .await.unwrap();
            result.insert("device-portal".to_string(), nodes);
            if result != local_cache {
                let dash: ConsulNodes = DashMap::from_iter(result.clone().into_iter());
                let _ = tx.send(dash).await.unwrap();
                local_cache = result;
            } else {
                println!("Consuul discovery is same");
            }
            sleep(Duration::from_secs(5)).await;
        }
    }
}


