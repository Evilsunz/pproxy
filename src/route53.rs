use anyhow::Error;
use async_trait::async_trait;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_route53::Config;
use aws_sdk_route53::config::Credentials;
use aws_sdk_route53::config::http::HttpResponse;
use aws_sdk_route53::error::SdkError;
use aws_sdk_route53::operation::change_resource_record_sets::{ChangeResourceRecordSetsError, ChangeResourceRecordSetsOutput};
use aws_sdk_route53::types::{Change, ChangeAction, ChangeBatch, ResourceRecord, ResourceRecordSet, RrType};
use pingora_core::server::ShutdownWatch;
use pingora_core::services::background::BackgroundService;
use crate::config::PPConfig;
use rand::rng;
use rand::seq::SliceRandom;
use tokio::runtime::Runtime;

use crate::lb::R53;
use crate::utils::resolve_ip;

impl R53 {

    pub fn new(pp_config: PPConfig) -> Self {
        Self {
            pp_config,
        }
    }

    pub fn non_async_r53_register(&self){
        println!("Registering r53...");
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            match register_ip_route53(&self.pp_config).await {
                Ok(_) => {}
                Err(err) => {
                    println!("{:?}", err);
                    std::process::exit(1);
                }
            };
        });
    }
}

#[async_trait]
impl BackgroundService for R53 {
    async fn start(&self, mut shutdown: ShutdownWatch) {
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    println!("Shutting down (dereg r53)...");
                    let _ = deregister_ip_route53(&self.pp_config).await;
                    break;
                }
            }
        }
    }
}

//TODO add error handle
//    0: InvalidChangeBatch: [Duplicate Resource Record: '133.0.0.13']
//     1: InvalidChangeBatch: [Duplicate Resource Record: '133.0.0.13']
//TODO + add jitter
pub async fn register_ip_route53(conf : &PPConfig) -> anyhow::Result<(), Error> {
    let ip = resolve_ip().await.unwrap_or_else(|_| panic!("Unable to resolve own IP - shutting down..."));
    let mut fqdns = conf.fqdns.clone();
    fqdns.shuffle(&mut rng());
    for fqdn in fqdns{
        process(conf.clone(), &fqdn, ip.as_ref(), add_res_record).await?;
    }
    Ok(())
}

//TODO add error handle
//TODO + add jitter
pub async fn deregister_ip_route53(conf : &PPConfig)-> anyhow::Result<(), Error>{
    let ip = resolve_ip().await.unwrap_or_else(|_| panic!("Unable to resolve own IP - shutting down..."));
    let mut fqdns = conf.fqdns.clone();
    fqdns.shuffle(&mut rng());
    for fqdn in fqdns{
        process(conf.clone(), &fqdn, ip.as_ref(), remove_res_record).await?;
    }
    Ok(())
}

async fn process<F>(conf : PPConfig, fqdn : &str, ip: &str, func: F) -> Result<ChangeResourceRecordSetsOutput,
                                                                               SdkError<ChangeResourceRecordSetsError, HttpResponse>>
where
    F: Fn(&str, Vec<ResourceRecord>) -> Vec<ResourceRecord>,
{
    let aws_access_key = conf.aws_access_key.clone();
    let aws_secret_key = conf.aws_secret_key.clone();
    let credentials = Credentials::new(aws_access_key, aws_secret_key, None, None, "custom-provider");

    let config = Config::builder()
        .credentials_provider(credentials)
        .region(Region::new("us-east-1"))
        .behavior_version(BehaviorVersion::latest())
        .build();

    let client = aws_sdk_route53::Client::from_conf(config);

    let response = client
        .list_resource_record_sets()
        .set_hosted_zone_id(Some(String::from(conf.r53_zone_id.clone())))
        .set_start_record_name(Some(fqdn.to_string()))
        .send()
        .await
        .unwrap();

    let existing_rr = response.resource_record_sets.get(0).unwrap().resource_records.clone().unwrap();



    let new_rr = func(ip, existing_rr);

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
        .hosted_zone_id(conf.r53_zone_id.clone())
        .set_change_batch(Some(change_batch))
        .send()
        .await
}

fn add_res_record(ip: &str, mut v: Vec<ResourceRecord>) -> Vec<ResourceRecord> {
    v.retain(|x| !x.value.eq(ip));
    v.push(ResourceRecord::builder().value(ip).build().unwrap());
    v
}

fn remove_res_record(ip: &str, mut v: Vec<ResourceRecord>) -> Vec<ResourceRecord> {
    v.retain(|x| !x.value.eq(ip));
    v
}
