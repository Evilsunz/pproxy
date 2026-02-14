use std::sync::Arc;
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{StatusCode};
use axum::response::Response;
use axum::{Json, Router};
use axum::routing::get;
use dashmap::DashMap;
use pingora_core::server::ShutdownWatch;
use pingora_core::services::background::BackgroundService;
use crate::config::PPConfig;
use crate::lb::{ConsulNode, Web};
use serde_json::{Value, json};
use crate::log_info;

#[async_trait]
impl BackgroundService for Web {
    async fn start(&self, mut shutdown: ShutdownWatch) {
        let self_clone = self.clone();
        let handle = tokio::spawn(async move { self_clone.bind_http().await });
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    handle.abort();
                    break;
                }
            }
        }
    }
}

impl Web {

    pub fn new(pp_config: PPConfig, nodes: Arc<DashMap<String, Vec<ConsulNode>>>) -> Self {
        Self {
            pp_config,
            nodes,
        }
    }

    pub async fn bind_http(&self) {
        let self_clone = self.clone();
        let router = Router::new()
            .route("/stats", get(move || async move { self_clone.stats().await }));
        log_info!("{}", format!("Listening on http://0.0.0.0:{} for stats endpoint", self.pp_config.port));
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}",self.pp_config.port)).await.unwrap();
        axum::serve(listener, router).await.unwrap();
    }

    async fn stats(&self) -> Json<Value> {

        let nodes = self
            .nodes
            .iter()
            .map(|entry| {
                let upstream = entry.key().clone();
                let endpoints: Vec<String> = entry
                    .value()
                    .iter()
                    .map(|n| format!("{}:{}", n.address, n.service_port))
                    .collect();
                (upstream, json!(endpoints))
            })
            .collect::<serde_json::Map<String, Value>>();
        
        Json(json!({
        "status": "OK",
        "nodes" : nodes
    }))
    }
    
}