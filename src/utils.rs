

const AWS_CHECK_IP_URL: &str = "http://checkip.amazonaws.com";

pub async fn resolve_ip() -> anyhow::Result<String> {
    let body = reqwest::get(AWS_CHECK_IP_URL).await?.text().await?;
    Ok(body.trim_end().to_string())
}