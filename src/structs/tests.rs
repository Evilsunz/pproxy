use super::*;

fn raw_entry(service_address: &str, node_address: &str, checks: Vec<ConsulCheckRaw>) -> ConsulEntryRaw {
    ConsulEntryRaw {
        node: ConsulNodeRaw {
            address: node_address.to_string(),
        },
        service: ConsulServiceRaw {
            name: "my-service".to_string(),
            address: service_address.to_string(),
            port: 8080,
        },
        checks,
    }
}

fn check(id: &str, output: &str) -> ConsulCheckRaw {
    ConsulCheckRaw {
        id: id.to_string(),
        status: "passing".to_string(),
        output: output.to_string(),
    }
}

#[test]
fn from_raw_uses_service_address_when_present() {
    let raw = raw_entry("10.0.0.10", "10.0.0.20", vec![]);

    let node = ConsulNode::from_raw(raw, false, "is leader", "ready", 10, 2);

    assert_eq!(node.service_name, "my-service");
    assert_eq!(node.address, "10.0.0.10");
    assert_eq!(node.service_port, 8080);
    assert_eq!(node.weight, 1);
}

#[test]
fn from_raw_falls_back_to_node_address_when_service_address_is_empty() {
    let raw = raw_entry("", "10.0.0.20", vec![]);

    let node = ConsulNode::from_raw(raw, false, "is leader", "ready", 10, 2);

    assert_eq!(node.address, "10.0.0.20");
    assert_eq!(node.weight, 1);
}

#[test]
fn from_raw_keeps_default_weight_when_weightening_is_disabled() {
    let raw = raw_entry(
        "10.0.0.10",
        "10.0.0.20",
        vec![check("health", "ready")],
    );

    let node = ConsulNode::from_raw(raw, false, "is leader", "ready", 10, 2);

    assert_eq!(node.weight, 1);
}

#[test]
fn from_raw_uses_weight_on_true_when_check_matches_condition() {
    let raw = raw_entry(
        "10.0.0.10",
        "10.0.0.20",
        vec![check("is leader", "{\"data\":true}\n")],
    );

    let node = ConsulNode::from_raw(raw, true, "is leader", "{\"data\":true}\n", 10, 2);

    assert_eq!(node.weight, 10);
}

#[test]
fn from_raw_uses_weight_on_false_when_check_does_not_match_condition() {
    let raw = raw_entry(
        "10.0.0.10",
        "10.0.0.20",
        vec![check("is leader", "")],
    );

    let node = ConsulNode::from_raw(raw, true, "is leader", "{\"data\":true}\n", 10, 2);

    assert_eq!(node.weight, 2);
}

#[test]
fn from_raw_keeps_default_weight_when_named_check_is_missing() {
    let raw = raw_entry(
        "10.0.0.10",
        "10.0.0.20",
        vec![check("other-check", "ready")],
    );

    let node = ConsulNode::from_raw(raw, true, "is leader", "ready", 10, 2);

    assert_eq!(node.weight, 1);
}