use std::collections::HashMap;
use serde_json::Value;

const CONSUL_URL: &str = "http://localhost:8500/";
const CONSUL_CREATE_SESSION: &str = "v1/session/create";
const CONSUL_RENEW_SESSION: &str = "v1/session/renew/";
const CONSUL_ACQUIRE_LOCK: &str = "v1/kv/service/pproxy/leader?acquire=";
const CONSUL_RELEASE_LOCK: &str = "v1/kv/service/pproxy/leader?release=";


fn create_consul_session() -> anyhow::Result<String> {
    //{"Name": "'`hostname`'", "TTL": "120s"}
    let mut payload = HashMap::new();
    payload.insert("Name", "pproxy");
    payload.insert("TTL", "1000s");
    let client = reqwest::blocking::Client::new();
    let response = client.put(format!("{}{}",CONSUL_URL,CONSUL_CREATE_SESSION)).json(&payload).send()?;
    let body = response.text()?;
    let map: HashMap<String, String> = serde_json::from_str(body.as_str())?;
    Ok(map.get("ID").unwrap().clone())
}

fn renew_consul_session(id : &str) -> anyhow::Result<HashMap<String,Value>> {
    let client = reqwest::blocking::Client::new();
    let response = client.put(format!("{}{}{}",CONSUL_URL,CONSUL_RENEW_SESSION, id)).send()?;
    let body = response.text()?;
    let vec: Vec<HashMap<String, Value>> = serde_json::from_str(body.as_str())?;
    Ok(vec.first().unwrap().clone())
}

fn acquire_consul_lock(id : &str, ip : &str) -> anyhow::Result<bool> {
    let mut payload = HashMap::new();
    payload.insert("Node", "pproxy");
    payload.insert("Ip", ip);
    let client = reqwest::blocking::Client::new();
    let response = client.put(format!("{}{}{}", CONSUL_URL, CONSUL_ACQUIRE_LOCK, id)).json(&payload).send()?;
    let body = response.text()?;
    let result : bool = body.parse()?;
    Ok(result)
}

fn release_consul_lock(id : &str) -> anyhow::Result<bool> {
    let mut payload = HashMap::new();
    payload.insert("Node", "pproxy");
    payload.insert("Ip", "0.0.0.0");
    let client = reqwest::blocking::Client::new();
    let response = client.put(format!("{}{}{}", CONSUL_URL, CONSUL_RELEASE_LOCK, id)).json(&payload).send()?;
    let body = response.text()?;
    let result : bool = body.parse()?;
    Ok(result)
}