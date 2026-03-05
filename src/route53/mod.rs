use crate::config::RPConfig;
use anyhow::Error;
use async_trait::async_trait;
use aws_sdk_route53::Client;
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

use crate::structs::{RuntimeState, R53};
use crate::utils::{get_res_record_sets, update_res_record_sets};
use crate::{log_error, log_info};

impl R53 {
    pub fn new(rp_config: RPConfig, runtime_state: RuntimeState) -> Self {
        Self { rp_config , runtime_state }
    }

    pub fn non_async_r53_register(&self) {
        log_info!("Registering r53...");
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let ip = self.runtime_state.ip.lock().unwrap().clone();
            let aws_r53_client = self
                .runtime_state
                .aws_r53_client
                .get()
                .expect("aws_r53_client not initialized");
            match register_ip_route53(&self.rp_config, &ip, aws_r53_client).await {
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
                    let ip = self.runtime_state.ip.lock().unwrap().clone();
                    let aws_r53_client = self
                        .runtime_state
                        .aws_r53_client
                        .get()
                        .expect("aws_r53_client not initialized");
                    match deregister_ip_route53(&self.rp_config, &ip, aws_r53_client).await {
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
pub async fn register_ip_route53(conf: &RPConfig, ip : &str, aws_r53_client: &Client,) -> anyhow::Result<(), Error> {
    let mut fqdns = conf.fqdns.clone();
    fqdns.shuffle(&mut rng());
    for fqdn in fqdns {
        process(conf.clone(), &fqdn, ip,aws_r53_client , add_res_record).await?;
    }
    Ok(())
}

//TODO add error handle
//TODO + add jitter
pub async fn deregister_ip_route53(conf: &RPConfig, ip : &str, aws_r53_client: &Client,) -> anyhow::Result<(), Error> {
    let mut fqdns = conf.fqdns.clone();
    fqdns.shuffle(&mut rng());
    for fqdn in fqdns {
        process(conf.clone(), &fqdn, ip, aws_r53_client ,remove_res_record).await?;
    }
    Ok(())
}

async fn process<F>(
    conf: RPConfig,
    fqdn: &str,
    ip: &str,
    aws_r53_client: &Client,
    func: F,
) -> Result<ChangeResourceRecordSetsOutput, SdkError<ChangeResourceRecordSetsError, HttpResponse>>
where
    F: Fn(&str, Vec<ResourceRecord>) -> Vec<ResourceRecord>,
{
    let response =
        get_res_record_sets(aws_r53_client, conf.r53_zone_id.clone(), fqdn.to_string()).await;
    let existing_rr = response
        .resource_record_sets
        .get(0)
        .unwrap()
        .resource_records
        .clone()
        .unwrap();

    let new_rr = func(ip, existing_rr);

    update_res_record_sets(
        aws_r53_client,
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
