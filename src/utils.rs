use crate::consul::VecConsulNode;

const AWS_CHECK_IP_URL: &str = "http://checkip.amazonaws.com";

pub fn resolve_ip() -> anyhow::Result<String> {
    let body = reqwest::blocking::get(AWS_CHECK_IP_URL)?.text()?;
    Ok(body.trim_end().to_string())
}

pub async fn get_consul_nodes(consul_url :&str, service_name: &str) -> anyhow::Result<VecConsulNode> {
    let nodes = reqwest::get(format!("{}{}{}", consul_url,"v1/catalog/service/", service_name))
        .await?.json::<VecConsulNode>()
        .await?;
    Ok(nodes)
}