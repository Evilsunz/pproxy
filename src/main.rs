mod lb;
mod consul;
mod config;

use pingora::prelude::*;
use std::sync::Arc;
use dashmap::DashMap;
use crate::lb::LB;


fn main() {
    env_logger::init();

    let mut my_server = Server::new(None).unwrap();
    my_server.bootstrap();

    // let upstreams =
    //     LoadBalancer::try_from_iter(["96.112.247.166:8443", "96.115.209.221:8443"]).unwrap();

    let lb = LB{
        nodes: Arc::new(DashMap::new()),
    };
    let consul_bg = background_service("consul-background", lb.clone());
    let mut lb = http_proxy_service(&my_server.configuration, lb);
    lb.add_tcp("0.0.0.0:6188");
    my_server.add_service(consul_bg);
    my_server.add_service(lb);
    println!("Server ready");
    my_server.run_forever();
}
