use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use async_trait::async_trait;
use pingora::prelude::sleep;
use pingora_core::server::ShutdownWatch;
use pingora_core::services::background::BackgroundService;
use serde_json::Value;
use crate::config::PPConfig;
use crate::lb::{LeaderRoutine};

const CONSUL_CREATE_SESSION: &str = "v1/session/create";
const CONSUL_RENEW_SESSION: &str = "v1/session/renew/";
const CONSUL_ACQUIRE_LOCK: &str = "v1/kv/service/pproxy/leader?acquire=";
const CONSUL_RELEASE_LOCK: &str = "v1/kv/service/pproxy/leader?release=";

#[async_trait]
impl BackgroundService for LeaderRoutine {
    async fn start(&self, mut shutdown: ShutdownWatch) {
        *self.session_id.lock().unwrap() = self.create_consul_session().await.unwrap();
        let self_clone = self.clone();
        let handle = tokio::spawn(async move { self_clone.routine().await });
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    let session_id = self.session_id.lock().unwrap().clone();
                    println!("Shutting down (releasing leader + Session id :{})...", session_id);
                    let _ = self.release_consul_lock(&session_id).await.unwrap();
                    handle.abort();
                    break;
                }
            }
        }
    }
}

impl LeaderRoutine {

    pub fn new(pp_config: PPConfig) -> Self {
        Self {
            pp_config,
            session_id: Arc::new(Mutex::new(String::new())),
        }
    }

    pub async fn routine(&self) {
        println!("Starting Leader routine...");
        let session_id = self.session_id.lock().unwrap().clone();
        let ip = self.pp_config.ip.clone().unwrap();
        loop{
            let leader = self.acquire_consul_lock(&session_id, ip.as_str()).await.unwrap();
            println!("Session id :{} + Leader : {}...", session_id, leader);
            if leader {
                //TODO
            }
            let _ = self.renew_consul_session(&session_id).await.unwrap();
            sleep(Duration::from_secs(self.pp_config.consul_leader_pool_secs)).await;
        }
    }

    async fn create_consul_session(&self) -> anyhow::Result<String> {
        //{"Name": "'`hostname`'", "TTL": "120s"}
        let mut payload = HashMap::new();
        payload.insert("Name", "pproxy");
        payload.insert("TTL", "1000s");
        let client = reqwest::Client::new();
        let response = client.put(format!("{}{}",self.pp_config.consul_url,CONSUL_CREATE_SESSION)).json(&payload).send().await?;
        let body = response.text().await?;
        let map: HashMap<String, String> = serde_json::from_str(body.as_str())?;
        Ok(map.get("ID").unwrap().clone())
    }

    async fn renew_consul_session(&self, id : &str) -> anyhow::Result<HashMap<String,Value>> {
        let client = reqwest::Client::new();
        let response = client.put(format!("{}{}{}",self.pp_config.consul_url ,CONSUL_RENEW_SESSION, id)).send().await?;
        let body = response.text().await?;
        let vec: Vec<HashMap<String, Value>> = serde_json::from_str(body.as_str())?;
        Ok(vec.first().unwrap().clone())
    }

    async fn acquire_consul_lock(&self, id : &str, ip : &str) -> anyhow::Result<bool> {
        let mut payload = HashMap::new();
        payload.insert("Node", "pproxy");
        payload.insert("Ip", ip);
        let client = reqwest::Client::new();
        let response = client.put(format!("{}{}{}", self.pp_config.consul_url, CONSUL_ACQUIRE_LOCK, id)).json(&payload).send().await?;
        let body = response.text().await?;
        let result : bool = body.parse()?;
        Ok(result)
    }

    async fn release_consul_lock(&self, id : &str) -> anyhow::Result<bool> {
        let mut payload = HashMap::new();
        payload.insert("Node", "pproxy");
        payload.insert("Ip", "0.0.0.0");
        let client = reqwest::Client::new();
        let response = client.put(format!("{}{}{}", self.pp_config.consul_url, CONSUL_RELEASE_LOCK, id)).json(&payload).send().await?;
        let body = response.text().await?;
        let result : bool = body.parse()?;
        println!("{result}");
        Ok(result)
    }

}