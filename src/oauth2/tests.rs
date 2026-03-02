use super::*;

impl AuthVerifier {
    pub fn new_for_tests(rp_config: RPConfig) -> Self {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let pub_path = format!("{manifest_dir}/config/jwt.pem");
        let priv_path = format!("{manifest_dir}/config/jwt_private.pem");

        let jwt_pub_pem = fs::read(&pub_path)
            .unwrap_or_else(|e| panic!("Failed to read test public key '{pub_path}': {e}"));
        let jwt_priv_pem = fs::read(&priv_path)
            .unwrap_or_else(|e| panic!("Failed to read test private key '{priv_path}': {e}"));

        let decoding_key =
            DecodingKey::from_rsa_pem(&jwt_pub_pem).expect("Invalid RSA public key PEM");
        let encoding_key =
            EncodingKey::from_rsa_pem(&jwt_priv_pem).expect("Invalid RSA private key PEM");

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&["rproxy"]);
        validation.set_audience(&["rproxy"]);

        let client = BasicClient::new(ClientId::new("".to_string()))
            .set_client_secret(ClientSecret::new("".to_string()))
            .set_auth_uri(AuthUrl::new("http://localhost".to_string()).expect("Invalid auth url"))
            .set_token_uri(
                TokenUrl::new("http://localhost".to_string()).expect("Invalid token url"),
            )
            .set_redirect_uri(
                RedirectUrl::new("http://localhost".to_string()).expect("Invalid redirect url"),
            );

        let http_client = oauth2::reqwest::ClientBuilder::new()
            .redirect(oauth2::reqwest::redirect::Policy::none())
            .build()
            .expect("Client should build");

        Self {
            rp_config,
            decoding_key,
            encoding_key,
            validation,
            client,
            http_client,
        }
    }
}

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
fn decide_auth_exchanges_when_code_query_param_is_present() {
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
fn decide_auth_proceeds_when_cookie_present_and_decodable() {
    let v = mock_verifier();
    let uri: Uri = "http://example.local/".parse().unwrap();

    let jwt = v.encode_jwt("xxx", "yyy").unwrap();

    let d = v.decide_auth(&uri, Some(&format!("rproxy_auth={}; other=1", jwt)));
    assert_eq!(d, AuthDecision::Proceed);
}

#[test]
fn decide_auth_redirects_when_cookie_present_jwt_malformed() {
    let v = mock_verifier();
    let uri: Uri = "http://example.local/".parse().unwrap();

    let d = v.decide_auth(
        &uri,
        Some(&format!("rproxy_auth=asdasdasdasdasdasd; other=1")),
    );
    assert_eq!(d, AuthDecision::RedirectToSso);
}
