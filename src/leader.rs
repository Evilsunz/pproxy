use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use async_trait::async_trait;
use aws_sdk_route53::types::ResourceRecord;
use pingora::prelude::sleep;
use pingora_core::server::ShutdownWatch;
use pingora_core::services::background::BackgroundService;
use serde_json::Value;
use crate::config::RPConfig;
use crate::lb::{LeaderRoutine};
use crate::{log_info, log_warn};
use crate::utils::{get_consul_nodes, get_res_record_sets, update_res_record_sets};

const CONSUL_CREATE_SESSION: &str = "v1/session/create";
const CONSUL_RENEW_SESSION: &str = "v1/session/renew/";
const CONSUL_ACQUIRE_LOCK: &str = "v1/kv/service/rproxy/leader?acquire=";
const CONSUL_RELEASE_LOCK: &str = "v1/kv/service/rproxy/leader?release=";

#[async_trait]
impl BackgroundService for LeaderRoutine {
    async fn start(&self, mut shutdown: ShutdownWatch) {
        *self.session_id.lock().unwrap() = self.create_consul_session().await.unwrap();
        let mut self_clone = self.clone();
        let handle = tokio::spawn(async move { self_clone.routine().await });
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    let session_id = self.session_id.lock().unwrap().clone();
                    log_info!("Shutting down (releasing leader + Session id :{})...", session_id);
                    let _ = self.release_consul_lock(&session_id).await.unwrap();
                    handle.abort();
                    break;
                }
            }
        }
    }
}

impl LeaderRoutine {

    pub fn new(rp_config: RPConfig) -> Self {
        Self {
            rp_config,
            session_id: Arc::new(Mutex::new(String::new())),
        }
    }

    pub async fn routine(&mut self) {
        log_info!("Starting Leader routine...");
        let session_id = self.session_id.lock().unwrap().clone();
        let ip = self.rp_config.ip.clone().unwrap();
        let client = self.rp_config.aws_r53_client.clone().unwrap();
        loop{
            let leader = self.acquire_consul_lock(&session_id, ip.as_str()).await.unwrap();
            log_info!("Session id :{} + Leader : {}...", session_id, leader);
            //todo set leader to rp_config
            if leader {
                self.rp_config.is_leader = Some(true);
                if let Ok(rproxies) = get_consul_nodes(self.rp_config.consul_url.as_str(), "rproxy").await
                {
                    let rproxy_ips: Vec<ResourceRecord> = rproxies.iter().map(|n| {
                        ResourceRecord::builder().set_value(Some(n.address.clone())).build().unwrap()
                    }).collect();
                    for fqdn in &self.rp_config.fqdns{
                        let response = get_res_record_sets(client.clone(), self.rp_config.r53_zone_id.clone(), fqdn.clone()).await;
                        let existing_rr = response.resource_record_sets.get(0).unwrap().resource_records.clone().unwrap();
                        let rez = compare_res_record(rproxy_ips.clone(), existing_rr.clone());
                        if !rez {
                            log_warn!("Found difference in r53 for fqdn {} : {:?} and pproxies ips {:?} .Resetting to pproxies_ips", fqdn, existing_rr, rproxy_ips);
                            let _ = update_res_record_sets(client.clone(), self.rp_config.r53_zone_id.clone(), fqdn.to_string(), rproxy_ips.clone()).await;
                        }
                    }
                }
            }
            let _ = self.renew_consul_session(&session_id).await.unwrap();
            sleep(Duration::from_secs(self.rp_config.consul_leader_pool_secs)).await;
        }
    }

    async fn create_consul_session(&self) -> anyhow::Result<String> {
        //{"Name": "'`hostname`'", "TTL": "120s"}
        let mut payload = HashMap::new();
        payload.insert("Name", "rproxy");
        payload.insert("TTL", "1000s");
        let client = reqwest::Client::new();
        let response = client.put(format!("{}{}", self.rp_config.consul_url, CONSUL_CREATE_SESSION)).json(&payload).send().await?;
        let body = response.text().await?;
        let map: HashMap<String, String> = serde_json::from_str(body.as_str())?;
        Ok(map.get("ID").unwrap().clone())
    }

    async fn renew_consul_session(&self, id : &str) -> anyhow::Result<HashMap<String,Value>> {
        let client = reqwest::Client::new();
        let response = client.put(format!("{}{}{}", self.rp_config.consul_url, CONSUL_RENEW_SESSION, id)).send().await?;
        let body = response.text().await?;
        let vec: Vec<HashMap<String, Value>> = serde_json::from_str(body.as_str())?;
        Ok(vec.first().unwrap().clone())
    }

    async fn acquire_consul_lock(&self, id : &str, ip : &str) -> anyhow::Result<bool> {
        let mut payload = HashMap::new();
        payload.insert("Node", "rproxy");
        payload.insert("Ip", ip);
        let client = reqwest::Client::new();
        let response = client.put(format!("{}{}{}", self.rp_config.consul_url, CONSUL_ACQUIRE_LOCK, id)).json(&payload).send().await?;
        let body = response.text().await?;
        let result : bool = body.parse()?;
        Ok(result)
    }

    async fn release_consul_lock(&self, id : &str) -> anyhow::Result<bool> {
        let mut payload = HashMap::new();
        payload.insert("Node", "rproxy");
        payload.insert("Ip", "0.0.0.0");
        let client = reqwest::Client::new();
        let response = client.put(format!("{}{}{}", self.rp_config.consul_url, CONSUL_RELEASE_LOCK, id)).json(&payload).send().await?;
        let body = response.text().await?;
        let result : bool = body.parse()?;
        Ok(result)
    }

}

fn compare_res_record(x: Vec<ResourceRecord>, v: Vec<ResourceRecord>) -> bool {
    let mut pproxies: Vec<String> = x
        .into_iter()
        .map(|rr| rr.value.clone())
        .collect();
    
    let mut record_set: Vec<String> = v
        .into_iter()
        .map(|rr| rr.value.clone())
        .collect();

    pproxies.sort();
    record_set.sort();

    pproxies == record_set
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_route53::types::ResourceRecord;

    fn rr(value: &str) -> ResourceRecord {
        ResourceRecord::builder()
            .set_value(Some(value.to_string()))
            .build()
            .expect("ResourceRecord build should succeed")
    }

    #[test]
    fn compare_res_record_equal_ignores_order() {
        let left = vec![rr("10.0.0.1"), rr("10.0.0.2"), rr("10.0.0.3")];
        let right = vec![rr("10.0.0.3"), rr("10.0.0.1"), rr("10.0.0.2")];

        assert!(compare_res_record(left, right));
    }

    #[test]
    fn compare_res_record_not_equal_when_values_differ() {
        let left = vec![rr("10.0.0.1"), rr("10.0.0.2")];
        let right = vec![rr("10.0.0.1"), rr("10.0.0.9")];

        assert!(!compare_res_record(left, right));
    }

    #[test]
    fn compare_res_record_not_equal_when_lengths_differ() {
        let left = vec![rr("10.0.0.1"), rr("10.0.0.2")];
        let right = vec![rr("10.0.0.1")];

        assert!(!compare_res_record(left, right));
    }

    #[test]
    fn compare_res_record_respects_multiplicity_of_duplicates() {
        let left = vec![rr("10.0.0.1"), rr("10.0.0.1"), rr("10.0.0.2")];
        let right = vec![rr("10.0.0.1"), rr("10.0.0.2"), rr("10.0.0.2")];

        assert!(!compare_res_record(left, right));
    }
}