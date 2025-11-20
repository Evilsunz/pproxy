mod lb;
mod consul;
mod config;

use std::path::PathBuf;
use pingora::prelude::*;
use std::sync::Arc;
use dashmap::DashMap;
use crate::config::{parse, PPConfig};
use crate::lb::LB;


fn main() {
    env_logger::init();

    let args = parse();
    let conf = match config::load(PathBuf::from(args.config_path)) {
        Ok(c) => {c}
        Err(e) => {panic!("Unable to load config : {}",e)}
    };
    
    println!("{:#?}", conf);
    
    let mut my_server = Server::new(None).unwrap();
    my_server.bootstrap();

    let lb = LB{
        nodes: Arc::new(DashMap::new()),
        balancers: Arc::new(DashMap::new()),
        pp_config: conf.clone()
    };
    let consul_bg = background_service("consul-background", lb.clone());
    let mut lb = http_proxy_service(&my_server.configuration, lb);
    lb.add_tcp(&format!("0.0.0.0:{}", conf.port));
    my_server.add_service(consul_bg);
    my_server.add_service(lb);
    println!("Server ready");
    my_server.run_forever();
}
