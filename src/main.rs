mod config;
mod consul;
mod leader;
mod logging;
mod oauth2;
mod proxy;
mod route53;
mod structs;
mod utils;
mod vault;
mod web;

use crate::config::parse;
use crate::logging::init_tracing;
use crate::structs::{LeaderRoutine, NetIqLoadBalancer, R53, Vault, Web, RuntimeState};
use pingora::prelude::*;
use std::path::PathBuf;
use crate::utils::{aws_r53_client, resolve_ip};

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() {
    let args = parse();
    let conf = match config::load(PathBuf::from(args.config_path)) {
        Ok(c) => c,
        Err(e) => {
            panic!("Unable to load config : {}", e)
        }
    };

    //init_log(conf.clone());

    let _guard = init_tracing(conf.clone());

    tracing::info!(target: "rproxy", "server starting");
    //+++ Runtime init +++ Move it ?
    let runtime_state = RuntimeState::new();
    let ip = resolve_ip().unwrap_or_else(|_| panic!("Unable to resolve own IP - shutting down..."));
    *runtime_state.ip.lock().unwrap() = ip;
    let aws_r53_client = aws_r53_client(conf.aws_access_key.clone(), conf.aws_secret_key.clone());
    runtime_state
        .aws_r53_client
        .set(aws_r53_client)
        .expect("aws_r53_client already initialized");
    //+++ Runtime init +++ Move it ? END

    let lb = NetIqLoadBalancer::new(conf.clone());
    let r53 = R53::new(conf.clone() , runtime_state.clone());
    let vault = Vault::new(conf.clone());
    let leader = LeaderRoutine::new(conf.clone(), runtime_state.clone());
    let web = Web::new(conf.clone(), lb.nodes.clone(), runtime_state.clone());

    r53.non_async_r53_register();

    let mut my_server = Server::new(Some(Opt::parse_args())).unwrap();
    my_server.bootstrap();

    let consul_bg = background_service("consul-background", lb.clone());
    let r53_bg = background_service("r53-background", r53);
    let leader_bg = background_service("leader-background", leader);
    let web_bg = background_service("web-background", web);

    let mut lb = http_proxy_service(&my_server.configuration, lb);
    //lb.add_tcp(&format!("0.0.0.0:{}", conf.port));

    if conf.tls_enabled {
        vault.non_async_fetch_ssl_certs();
        let cert_path = conf.tls_chain_cert.clone();
        let key_path = conf.tls_private_cert.clone();
        let mut tls_settings =
            pingora_core::listeners::tls::TlsSettings::intermediate(&cert_path, &key_path).unwrap();
        if conf.tls_enable_h2{
            tls_settings.enable_h2();
        }
        lb.add_tls_with_settings(&format!("0.0.0.0:{}", conf.tls_port), None, tls_settings);
    }
    my_server.add_service(consul_bg);
    my_server.add_service(r53_bg);
    my_server.add_service(leader_bg);
    my_server.add_service(web_bg);
    my_server.add_service(lb);
    log_info!("Server ready");
    my_server.run_forever();
}
