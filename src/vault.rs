use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use pem::parse_many;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use vaultrs::client::{VaultClient, VaultClientSettingsBuilder};
use vaultrs::kv2;
use vaultrs_login::engines::approle::AppRoleLogin;
use vaultrs_login::LoginClient;
use crate::config::RPConfig;
use anyhow::{Error, Result};
use tokio::runtime::Runtime;
use tokio_retry::Retry;
use crate::lb::Vault;
use crate::{log_error, log_info};

impl Vault {

    pub fn new(rp_config: RPConfig) -> Self {
        Self {
            rp_config,
        }
    }
    
    pub fn non_async_fetch_ssl_certs(&self){
        log_info!("Fetching certs...");
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            match fetch_ssl_certs(&self.rp_config).await {
                Ok(_) => {}
                Err(err) => {
                    log_error!("{:?}", err);
                    std::process::exit(1);
                }
            };
        });
    }
}

async fn fetch_ssl_certs(conf : &RPConfig) -> Result<(), Error> {
    let retry_strategy = ExponentialBackoff::from_millis(10)
        .map(jitter)
        .take(4);

    Retry::spawn(retry_strategy, move || internal_fetch_ssl_certs(conf)).await
}

async fn internal_fetch_ssl_certs(conf: &RPConfig) -> Result<(), Error> {

    let mut client = VaultClient::new(
        VaultClientSettingsBuilder::default()
            .address(conf.vault_address.clone())
            .build()?
    )?;

    let role_id = conf.role_id.clone();
    let secret_id = conf.secret_id.clone();
    let login = AppRoleLogin { role_id, secret_id };

    client.login("approle", &login).await?;

    let full_cert : HashMap<String,String>= kv2::read(&client, "kv2", &conf.path_to_cert_secret.clone()).await?;

    let vec = BASE64_STANDARD.decode(full_cert.get("data").unwrap())?;
    let pem = parse_many(vec)?;
    
    //Writing private [0] cert to separate a file
    std::fs::write(conf.tls_private_cert.clone(), pem[0].clone().to_string())?;
    
    //Writing another cert chain [1..] to separate a file
    let mut f = File::create(conf.tls_chain_cert.clone())?;
    for i in pem[1..].iter() {
        f.write_all(i.to_string().as_ref())?;
    }
    log_info!("Certificate updated...");
    Ok(())
}