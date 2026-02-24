use crate::config::RPConfig;
use anyhow::Error;
use async_trait::async_trait;
use aws_sdk_route53::config::http::HttpResponse;
use aws_sdk_route53::error::SdkError;
use aws_sdk_route53::operation::change_resource_record_sets::{
    ChangeResourceRecordSetsError, ChangeResourceRecordSetsOutput,
};
use aws_sdk_route53::types::ResourceRecord;
use pingora_core::server::ShutdownWatch;
use pingora_core::services::background::BackgroundService;
use rand::rng;
use rand::seq::SliceRandom;
use tokio::runtime::Runtime;

use crate::lb::R53;
use crate::utils::{get_res_record_sets, update_res_record_sets};
use crate::{log_error, log_info};

impl R53 {
    pub fn new(rp_config: RPConfig) -> Self {
        Self { rp_config }
    }

    pub fn non_async_r53_register(&self) {
        log_info!("Registering r53...");
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            match register_ip_route53(&self.rp_config).await {
                Ok(_) => {}
                Err(err) => {
                    log_error!("{:?}", err);
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
                    log_info!("Shutting down (dereg r53)...");
                    match deregister_ip_route53(&self.rp_config).await {
                        Ok(_) => {}
                        Err(err) => {
                            log_error!("{:?}", err);
                        }
                    };
                    break;
                }
            }
        }
    }
}

//TODO + add jitter
pub async fn register_ip_route53(conf: &RPConfig) -> anyhow::Result<(), Error> {
    let ip = conf.ip.as_ref().unwrap();
    let mut fqdns = conf.fqdns.clone();
    fqdns.shuffle(&mut rng());
    for fqdn in fqdns {
        process(conf.clone(), &fqdn, ip.as_ref(), add_res_record).await?;
    }
    Ok(())
}

//TODO add error handle
//TODO + add jitter
pub async fn deregister_ip_route53(conf: &RPConfig) -> anyhow::Result<(), Error> {
    let ip = conf.ip.as_ref().unwrap();
    let mut fqdns = conf.fqdns.clone();
    fqdns.shuffle(&mut rng());
    for fqdn in fqdns {
        process(conf.clone(), &fqdn, ip.as_ref(), remove_res_record).await?;
    }
    Ok(())
}

async fn process<F>(
    conf: RPConfig,
    fqdn: &str,
    ip: &str,
    func: F,
) -> Result<ChangeResourceRecordSetsOutput, SdkError<ChangeResourceRecordSetsError, HttpResponse>>
where
    F: Fn(&str, Vec<ResourceRecord>) -> Vec<ResourceRecord>,
{
    let client = conf.aws_r53_client.as_ref().unwrap();
    let response =
        get_res_record_sets(client.clone(), conf.r53_zone_id.clone(), fqdn.to_string()).await;
    let existing_rr = response
        .resource_record_sets
        .get(0)
        .unwrap()
        .resource_records
        .clone()
        .unwrap();

    let new_rr = func(ip, existing_rr);

    update_res_record_sets(
        client.clone(),
        conf.r53_zone_id.clone(),
        fqdn.to_string(),
        new_rr,
    )
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
