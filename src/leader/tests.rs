use super::*;
use aws_sdk_route53::types::ResourceRecord;

fn rr(value: &str) -> ResourceRecord {
    ResourceRecord::builder()
        .set_value(Some(value.to_string()))
        .build()
        .expect("ResourceRecord build should succeed")
}

#[test]
fn compare_res_record_equal_ignores_order() {
    let left = vec![rr("10.0.0.1"), rr("10.0.0.2"), rr("10.0.0.3")];
    let right = vec![rr("10.0.0.3"), rr("10.0.0.1"), rr("10.0.0.2")];

    assert!(compare_res_record(left, right));
}

#[test]
fn compare_res_record_not_equal_when_values_differ() {
    let left = vec![rr("10.0.0.1"), rr("10.0.0.2")];
    let right = vec![rr("10.0.0.1"), rr("10.0.0.9")];

    assert!(!compare_res_record(left, right));
}

#[test]
fn compare_res_record_not_equal_when_lengths_differ() {
    let left = vec![rr("10.0.0.1"), rr("10.0.0.2")];
    let right = vec![rr("10.0.0.1")];

    assert!(!compare_res_record(left, right));
}

#[test]
fn compare_res_record_respects_multiplicity_of_duplicates() {
    let left = vec![rr("10.0.0.1"), rr("10.0.0.1"), rr("10.0.0.2")];
    let right = vec![rr("10.0.0.1"), rr("10.0.0.2"), rr("10.0.0.2")];

    assert!(!compare_res_record(left, right));
}
