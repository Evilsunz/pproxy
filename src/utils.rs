use aws_config::{BehaviorVersion, Region};
use aws_sdk_route53::{Client, Config};
use aws_sdk_route53::config::Credentials;
use aws_sdk_route53::config::http::HttpResponse;
use aws_sdk_route53::error::SdkError;
use aws_sdk_route53::operation::change_resource_record_sets::{ChangeResourceRecordSetsError, ChangeResourceRecordSetsOutput};
use aws_sdk_route53::operation::list_resource_record_sets::ListResourceRecordSetsOutput;
use aws_sdk_route53::types::{Change, ChangeAction, ChangeBatch, ResourceRecord, ResourceRecordSet, RrType};
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
    if nodes.is_empty() {
        return Err(anyhow::anyhow!("No consul node found"));
    }

    Ok(nodes)
}

pub async fn get_res_record_sets(client: Client, r53_zone_id : String, fqdn : String) -> ListResourceRecordSetsOutput {
    client
        .list_resource_record_sets()
        .set_hosted_zone_id(Some(r53_zone_id))
        .set_start_record_name(Some(fqdn))
        .send()
        .await.expect("Unable to request R53 records")
}

pub async fn update_res_record_sets(client: Client, r53_zone_id : String, fqdn : String, new_rr : Vec<ResourceRecord>) ->Result<ChangeResourceRecordSetsOutput,
    SdkError<ChangeResourceRecordSetsError, HttpResponse>> {
    let resource_record_set = ResourceRecordSet::builder()
        .name(fqdn.to_string())
        .r#type(RrType::A)
        .ttl(300)
        .set_resource_records(Some(new_rr))
        .build()
        .unwrap();

    let change = Change::builder()
        .action(ChangeAction::Upsert)
        .resource_record_set(resource_record_set)
        .build()
        .unwrap();

    let change_batch = ChangeBatch::builder().changes(change).build().unwrap();

    client
        .change_resource_record_sets()
        .hosted_zone_id(r53_zone_id)
        .set_change_batch(Some(change_batch))
        .send()
        .await
}



pub fn aws_r53_client(aws_access_key : String, aws_secret_key : String) -> Client{
    let credentials = Credentials::new(aws_access_key, aws_secret_key, None, None, "custom-provider");

    let config = Config::builder()
        .credentials_provider(credentials)
        .region(Region::new("us-east-1"))
        .behavior_version(BehaviorVersion::latest())
        .build();

    Client::from_conf(config)
}