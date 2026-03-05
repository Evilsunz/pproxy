use crate::config::RPConfig;
use crate::log_info;
use crate::structs::{ConsulNode, RuntimeState, Web};
use async_trait::async_trait;
use axum::response::Redirect;
use axum::routing::get;
use axum::{Json, Router};
use dashmap::DashMap;
use pingora_core::server::ShutdownWatch;
use pingora_core::services::background::BackgroundService;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::Ordering;

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
    pub fn new(rp_config: RPConfig, nodes: Arc<DashMap<String, Vec<ConsulNode>>>, runtime_state: RuntimeState) -> Self {
        Self { rp_config, nodes , runtime_state }
    }

    pub async fn bind_http(&self) {
        let self_clone = self.clone();
        let router = Router::new()
            .route("/", get(|| async { Redirect::permanent("/stats") }))
            .route(
                "/stats",
                get(move || async move { self_clone.stats().await }),
            );
        log_info!(
            "{}",
            format!(
                "Listening on http://0.0.0.0:{} for stats endpoint",
                self.rp_config.port
            )
        );
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", self.rp_config.port))
            .await
            .unwrap();
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
            "leader": self.runtime_state.is_leader.load(Ordering::Relaxed),
            "nodes" : nodes
        }))
    }
}
