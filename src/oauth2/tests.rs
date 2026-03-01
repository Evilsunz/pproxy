
use super::*;

fn mock_verifier() -> AuthVerifier {
    AuthVerifier::new_for_tests(RPConfig::default())
}

#[test]
fn decide_auth_redirects_when_no_cookie_header() {
    let v = mock_verifier();
    let uri: Uri = "http://example.local/".parse().unwrap();

    let d = v.decide_auth(&uri, None);
    assert_eq!(d, AuthDecision::RedirectToSso);
}

#[test]
fn decide_auth_redirects_when_cookie_missing() {
    let v = mock_verifier();
    let uri: Uri = "http://example.local/".parse().unwrap();

    let d = v.decide_auth(&uri, Some("other=1; something=2"));
    assert_eq!(d, AuthDecision::RedirectToSso);
}

#[test]
fn decide_auth_redirects_when_code_query_param_is_present() {
    let v = mock_verifier();
    let uri: Uri = "http://example.local/?code=abababbsdkajsdlkasl"
        .parse()
        .unwrap();

    let d = v.decide_auth(&uri, None);
    assert_eq!(
        d,
        AuthDecision::Exchange {
            code: "abababbsdkajsdlkasl".to_string()
        }
    );
}

#[test]
fn decide_auth_proceeds_when_cookie_present() {
    let v = mock_verifier();
    let uri: Uri = "http://example.local/".parse().unwrap();

    let jwt = v.encode_jwt("xxx", "yyy").unwrap();

    let d = v.decide_auth(&uri, Some(&format!("rproxy_auth={}; other=1", jwt)));
    assert_eq!(d, AuthDecision::Proceed);
}

#[test]
fn decide_auth_proceeds_when_cookie_present_jwt_malformed() {
    let v = mock_verifier();
    let uri: Uri = "http://example.local/".parse().unwrap();

    let d = v.decide_auth(
        &uri,
        Some(&format!("rproxy_auth=asdasdasdasdasdasd; other=1")),
    );
    assert_eq!(d, AuthDecision::RedirectToSso);
}
