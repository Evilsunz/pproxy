mod lb;
mod consul;
mod config;
mod vault;
mod route53;
mod utils;

use std::path::PathBuf;
use pingora::prelude::*;
use std::sync::Arc;
use dashmap::DashMap;
use crate::config::{parse};
use crate::lb::LB;
use crate::route53::{R53ShutdownWatch};
use crate::vault::non_async_fetch_ssl_certs;

fn main() {
    //env_logger::init();

    let args = parse();
    let conf = match config::load(PathBuf::from(args.config_path)) {
        Ok(c) => {c}
        Err(e) => {panic!("Unable to load config : {}",e)}
    };
    
    println!("{:#?}", conf);

    let lb = LB{
        nodes: Arc::new(DashMap::new()),
        balancers: Arc::new(DashMap::new()),
        pp_config: conf.clone()
    };

    let r53 = R53ShutdownWatch{
        pp_config: conf.clone()
    };
    
    non_async_fetch_ssl_certs(&conf);
    r53.non_async_r53_register();
    
    let mut my_server = Server::new(None).unwrap();
    my_server.bootstrap();
    
    let consul_bg = background_service("consul-background", lb.clone());
    let r53_bg = background_service("r53-background", r53.clone());
    let mut lb = http_proxy_service(&my_server.configuration, lb);
    lb.add_tcp(&format!("0.0.0.0:{}", conf.port));

    let cert_path = conf.tls_chain_cert.clone();
    let key_path = conf.tls_private_cert.clone();

    let mut tls_settings =
        pingora_core::listeners::tls::TlsSettings::intermediate(&cert_path, &key_path).unwrap();
    tls_settings.enable_h2();
    lb.add_tls_with_settings(&format!("0.0.0.0:{}" , conf.tls_port), None, tls_settings);

    my_server.add_service(consul_bg);
    my_server.add_service(r53_bg);
    my_server.add_service(lb);
    println!("Server ready");
    my_server.run_forever();
}
