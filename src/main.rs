mod lb;
mod consul;
mod config;
mod vault;
mod route53;
mod utils;
mod proxy;
mod leader;
mod web;
mod logging;
mod oauth2;

use std::fs;
use std::path::{Path, PathBuf};
use jsonwebtoken::{decode, encode, Algorithm, EncodingKey, Header};
use jsonwebtoken::crypto::CryptoProvider;
use pingora::prelude::*;
use crate::config::parse;
use crate::lb::{R53, NetIqLoadBalancer, Vault, LeaderRoutine, Web, AuthVerifier, AuthClaims};
use crate::logging::init_log;

fn main() {

    let args = parse();
    let conf = match config::load(PathBuf::from(args.config_path)) {
        Ok(c) => {c}
        Err(e) => {panic!("Unable to load config : {}",e)}
    };

    init_log(conf.clone());

    let lb = NetIqLoadBalancer::new(conf.clone());
    let r53 = R53::new(conf.clone());
    let vault = Vault::new(conf.clone());
    let leader = LeaderRoutine::new(conf.clone());
    let web = Web::new(conf.clone(), lb.nodes.clone());

    r53.non_async_r53_register();

    let mut my_server = Server::new(Some(Opt::parse_args())).unwrap();
    my_server.bootstrap();

    let consul_bg = background_service("consul-background", lb.clone());
    let r53_bg = background_service("r53-background", r53);
    let leader_bg = background_service("leader-background", leader);
    let web_bg = background_service("web-background", web);

    let mut  lb = http_proxy_service(&my_server.configuration, lb);
    //lb.add_tcp(&format!("0.0.0.0:{}", conf.port));

    if conf.tls_enabled {
        vault.non_async_fetch_ssl_certs();
        let cert_path = conf.tls_chain_cert.clone();
        let key_path = conf.tls_private_cert.clone();
        let mut tls_settings =
            pingora_core::listeners::tls::TlsSettings::intermediate(&cert_path, &key_path).unwrap();
        tls_settings.enable_h2();
        lb.add_tls_with_settings(&format!("0.0.0.0:{}" , conf.tls_port), None, tls_settings);
    }
    my_server.add_service(consul_bg);
    my_server.add_service(r53_bg);
    my_server.add_service(leader_bg);
    my_server.add_service(web_bg);
    my_server.add_service(lb);
    log_info!("Server ready");
    my_server.run_forever();
}