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
use crate::config::PPConfig;
use anyhow::{Context, Error, Result};
use tokio::runtime::Runtime;
use tokio_retry::Retry;

pub fn non_async_fetch_ssl_certs(conf : &PPConfig){
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        match fetch_ssl_certs(&conf).await {
            Ok(_) => {}
            Err(err) => {
                println!("{:?}", err);
                std::process::exit(1);
            }
        };
    });
}

async fn fetch_ssl_certs(conf : &PPConfig) -> Result<(), Error> {
    let retry_strategy = ExponentialBackoff::from_millis(10)
        //.map(jitter)
        .take(1);

    let res = Retry::spawn(retry_strategy, move || internal_fetch_ssl_certs(&conf)).await;
    //let res = internal_fetch_ssl_certs(&conf).await;
    res
}

async fn internal_fetch_ssl_certs(conf: &PPConfig) -> Result<(), Error> {

    let mut client = VaultClient::new(
        VaultClientSettingsBuilder::default()
            .address(conf.vault_address.clone())
            .build()?
    )?;

    let role_id = conf.role_id.clone();
    let secret_id = conf.secret_id.clone();
    let login = AppRoleLogin { role_id, secret_id };

    let _ =client.login("approle", &login).await?;

    let full_cert : HashMap<String,String>= kv2::read(&client, "kv2", &conf.path_to_cert_secret.clone()).await?;

    let vec = BASE64_STANDARD.decode(full_cert.get("data").unwrap())?;
    let pem = parse_many(vec).expect("Unable to parse base64 full chain pem");
    
    //Writing priv cert to separate file
    std::fs::write(conf.tls_private_cert.clone(), pem[0].clone().to_string())?;
    let mut f = File::create(conf.tls_chain_cert.clone()).expect("Unable to create file");
    
    //Writing other cert chain to separate file
    for i in pem[1..].iter() {
        f.write_all(i.to_string().as_ref())?;
    }
    Ok(())
}