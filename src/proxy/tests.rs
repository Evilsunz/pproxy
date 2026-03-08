use pingora::lb::{Backend, Backends, LoadBalancer};
use pingora::prelude::RoundRobin;
use std::collections::{BTreeMap, BTreeSet};
use pingora::lb::discovery::Static;

#[tokio::test]
async fn weighted_backends_are_selected_proportionally() {
    let mut set = BTreeSet::new();

    let mut b1 = Backend::new("127.0.0.1:10001").unwrap();
    b1.weight = 1;
    set.insert(b1);

    let mut b2 = Backend::new("127.0.0.1:10002").unwrap();
    b2.weight = 2;
    set.insert(b2);

    let backends = Backends::new(Static::new(set));
    let lb = LoadBalancer::<RoundRobin>::from_backends(backends);

    lb.update().await.unwrap();

    let mut counts = BTreeMap::new();

    for _ in 0..10_000 {
        let backend = lb.select(b"", 256).expect("backend should be selected");
        let key = backend.to_string();
        *counts.entry(key).or_insert(0usize) += 1;
    }

    assert!(counts.entry("127.0.0.1:10001".to_string()).or_default() < &mut 3500);
    assert!(counts.entry("127.0.0.1:10002".to_string()).or_default() > &mut 6500);
    println!("{counts:#?}");

}