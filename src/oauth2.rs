use crate::config::RPConfig;
use crate::lb::{AuthClaims, AuthDecision, AuthVerifier};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use oauth2::basic::BasicClient;
use oauth2::http::Uri;
use oauth2::{AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope, TokenUrl};
use pingora::http::{ResponseHeader, StatusCode};
use pingora::prelude::Session;
use std::fs;
use oauth2::url::Url;
use crate::{log_error, log_trace};

const COOKIE_NAME: &str = "rproxy_auth";
const ISSUER: &str = "rproxy";
const COOKIE_HEADER_NAME: &str = "Cookie";

impl AuthVerifier {

    pub fn new(rp_config: RPConfig) -> Self {
        let jwt_pub_pem = fs::read(&rp_config.jwt_cert)
            .unwrap_or_else(|e| panic!("Failed to read jwt_cert PEM file '{}': {e}", &rp_config.jwt_cert));

        let jwt_priv_pem = fs::read(&rp_config.jwt_private_cert)
            .unwrap_or_else(|e| panic!("Failed to read jwt_private_cert PEM file '{}': {e}", &rp_config.jwt_private_cert));

        let decoding_key: DecodingKey = match DecodingKey::from_rsa_pem(&jwt_pub_pem) {
            Ok(key) => key,
            Err(err) => panic!("Failed to create DecodingKey: {}", err),
        };
        let encoding_key = match EncodingKey::from_rsa_pem(&jwt_priv_pem) {
            Ok(key) => key,
            Err(err) => panic!("Failed to create EncodingKey: {}", err),
        };

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&["rproxy"]);
        validation.set_audience(&["rproxy"]);

        Self {
            rp_config,
            decoding_key,
            encoding_key,
            validation,
        }
    }

    #[cfg(test)]
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

        Self {
            rp_config,
            decoding_key,
            encoding_key,
            validation,
        }
    }

    pub async fn verify_auth_cookie(&self, session: &mut Session) -> pingora::Result<bool> {
        log_trace!("Uri host{}", session.req_header().uri);

        let cookie_header = session
            .get_header(COOKIE_HEADER_NAME)
            .and_then(|h| h.to_str().ok());

        match self.decide_auth(&session.req_header().uri, cookie_header) {
            AuthDecision::Exchange { code } => self.exchange(&code).await,
            AuthDecision::RedirectToSso => self.redirect_to_sso(session).await,
            AuthDecision::Proceed=>  {Ok(false)},
        }
    }

    fn decide_auth(&self, uri: &Uri, cookie_header: Option<&str>) -> AuthDecision {
        if let Some(code) = self.is_oauth_redirect_with_code(uri) {
            return AuthDecision::Exchange { code };
        }

        let Some(cookie_header) = cookie_header
        else {
            return AuthDecision::RedirectToSso;
        };

        let Some(jwt) = self.is_have_cookie_value_by_name(cookie_header, COOKIE_NAME)
        else {
            return AuthDecision::RedirectToSso;
        };

        if self.decode_jwt(&jwt).is_err() {
            return AuthDecision::RedirectToSso;
        }

        AuthDecision::Proceed
    }

    async fn redirect_to_sso(&self, session: &mut Session) -> pingora::Result<bool> {
        log_trace!("Redirecting to SSO + req summary {}", session.request_summary());

        let location = match self.get_redirect_url() {
            Ok(url) => url,
            Err(e) => {
                log_error!("Got error during constructing redirect url {}", e);
                return Ok(true);
            }
        };

        let mut resp = ResponseHeader::build(StatusCode::FOUND, Some(0))?;
        resp.insert_header("Location", location)?;
        session.write_response_header(Box::new(resp), true).await?;
        Ok(true)
    }

    fn is_oauth_redirect_with_code(&self, uri : &Uri) -> Option<String> {
        const HOST_PREFIX: &str = "http://localhost/";
        let url = &(HOST_PREFIX.to_string() + &uri.to_string());
        let Ok(parsed_url) = Url::parse(url)
        else {
            log_error!("Failed to parse URL: {}", url);
            return None;
        };
        parsed_url
            .query_pairs()
            .find(|(key, _)| key == "code")
            .map(|(_, code)| AuthorizationCode::new(code.into_owned()))
            .map(|code| code.secret().to_owned())
    }

    async fn exchange(&self, code: &str) -> pingora::Result<bool> {
        //make exchange - if ok - encode jwt - return false -> proceed
        //                       else resp with nont uth response - > true
        let _ = self.encode_jwt("","");
        Ok(true)
    }

    fn is_have_cookie_value_by_name(&self, cookie_header: &str, name: &str) -> Option<String> {
        for part in cookie_header.split(';') {
            let part = part.trim();
            if let Some(v) = part.strip_prefix(&format!("{name}=")) {
                return Some(v.to_string());
            }
        }
        None
    }

    fn decode_jwt(&self, cookie_value: &str) -> anyhow::Result<AuthClaims> {
        Ok(decode::<AuthClaims>(cookie_value, &self.decoding_key, &self.validation)?.claims)
    }

    fn encode_jwt(&self, sub: &str, tid: &str) -> anyhow::Result<String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        let claims = AuthClaims {
            sub: sub.to_string(),
            tid: tid.to_string(),
            exp: now + (60 * 60 * 24),
            //TODO HARDCODE
            iat: now,
            iss: ISSUER.to_string(),
            aud: ISSUER.to_string(),
        };

        Ok(encode(
            &Header::new(Algorithm::RS256),
            &claims,
            &self.encoding_key,
        )?)
    }

    fn get_redirect_url(&self) -> anyhow::Result<String> {
        //TODO move to init ?
        let client = BasicClient::new(ClientId::new(self.rp_config.client_id.clone()))
            .set_client_secret(ClientSecret::new(self.rp_config.client_secret.clone()))
            .set_auth_uri(AuthUrl::new(self.rp_config.auth_url.clone())?)
            .set_token_uri(TokenUrl::new(self.rp_config.token_url.clone())?)
            .set_redirect_uri(RedirectUrl::new(self.rp_config.redirect_url.clone())?);

        let (auth_url, _) = client
            .authorize_url(CsrfToken::new_random)
            .add_scopes(self.rp_config.scopes.iter().map(|s| Scope::new(s.to_string())))
            //.set_pkce_challenge(pkce_challenge)
            .url();

        Ok(auth_url.to_string())
    }

}

#[cfg(test)]
mod tests {
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
        let uri: Uri = "http://example.local/?code=abababbsdkajsdlkasl".parse().unwrap();

        let d = v.decide_auth(&uri, None);
        assert_eq!(d, AuthDecision::Exchange { code: "abababbsdkajsdlkasl".to_string() });
    }

    #[test]
    fn decide_auth_proceeds_when_cookie_present() {
        let v = mock_verifier();
        let uri: Uri = "http://example.local/".parse().unwrap();
        
        let jwt = v.encode_jwt("xxx","yyy").unwrap();

        let d = v.decide_auth(&uri, Some(&format!("rproxy_auth={}; other=1", jwt)));
        assert_eq!(d, AuthDecision::Proceed);
    }

    #[test]
    fn decide_auth_proceeds_when_cookie_present_jwt_malformed() {
        let v = mock_verifier();
        let uri: Uri = "http://example.local/".parse().unwrap();

        let d = v.decide_auth(&uri, Some(&format!("rproxy_auth=asdasdasdasdasdasd; other=1")));
        assert_eq!(d, AuthDecision::RedirectToSso);
    }

}
