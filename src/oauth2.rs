use crate::config::RPConfig;
use crate::lb::{AuthClaims, AuthVerifier};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use oauth2::basic::BasicClient;
use oauth2::http::Uri;
use oauth2::{AuthUrl, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope, TokenUrl};
use pingora::http::{ResponseHeader, StatusCode};
use pingora::prelude::Session;
use std::fs;

use crate::{log_error, log_trace};

const COOKIE_NAME: &str = "rproxy_auth";
const ISSUER: &str = "rproxy";
const COOKIE_HEADER_NAME: &str = "Cookie";

// -------------------- NEW: pure decision layer --------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum AuthDecision {
    Exchange { code: String },
    RedirectToSso,
    Proceed { jwt: String },
}

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
        let secret = b"test-secret-placeholder";
        let decoding_key = DecodingKey::from_secret(secret);
        let encoding_key = EncodingKey::from_secret(secret);

        let validation = Validation::new(Algorithm::HS256);

        Self {
            rp_config,
            decoding_key,
            encoding_key,
            validation,
        }
    }

    fn decide_auth(&self, uri: &Uri, cookie_header: Option<&str>) -> AuthDecision {
        if let Some(code) = self.is_oauth_redirect_with_code(uri) {
            return AuthDecision::Exchange { code };
        }

        let cookie_header = match cookie_header {
            Some(v) => v,
            None => return AuthDecision::RedirectToSso,
        };

        let jwt = match self.get_cookie_value(cookie_header, COOKIE_NAME) {
            Some(v) => v,
            None => return AuthDecision::RedirectToSso,
        };

        AuthDecision::Proceed { jwt }
    }

    // -------------------- existing code --------------------

    pub async fn verify_auth_cookie(&self, session: &mut Session) -> anyhow::Result<bool> {
        log_trace!("Uri host{}", session.req_header().uri);

        let cookie_header = session
            .get_header(COOKIE_HEADER_NAME)
            .and_then(|h| h.to_str().ok());

        match self.decide_auth(&session.req_header().uri, cookie_header) {
            AuthDecision::Exchange { code } => self.exchange(&code),

            AuthDecision::RedirectToSso => self.redirect_to_sso(session).await,

            AuthDecision::Proceed { jwt } => match self.decode_jwt(&jwt) {
                Ok(_) => Ok(false),//pass
                Err(_) => self.redirect_to_sso(session).await,
            },
        }
    }

    async fn redirect_to_sso(&self, session: &mut Session) -> anyhow::Result<bool> {
        println!("Redirecting to SSO + req summary {}", session.request_summary());
        println!();

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
        Some("".to_string())
    }

    fn exchange(&self, cookie_header: &str) -> anyhow::Result<bool> {
        //make exchange - if ok - encode jwt - return false -> proceed
        //                       else resp with nont uth response - > true
        let _ = self.encode_jwt("","");
        Ok(true)
    }

    fn get_cookie_value(&self, cookie_header: &str, name: &str) -> Option<String> {
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

        //let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let (auth_url, _) = client
            .authorize_url(CsrfToken::new_random)
            .add_scopes(self.rp_config.scopes.iter().map(|s| Scope::new(s.to_string())))
            //.set_pkce_challenge(pkce_challenge)
            .url();

        Ok(auth_url.to_string())
    }

}

// -------------------- NEW: unit tests for pure logic --------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Т.к. decide_auth использует только is_oauth_redirect_with_code + get_cookie_value,
    // можно создать "минимальный" AuthVerifier. Если ваш new() читает PEM и мешает тестам,
    // то добавьте отдельный new_for_tests или создавайте verifier там, где ключи не нужны.
    fn verifier_minimal() -> AuthVerifier {
        // Если тут упираетесь в чтение pem — скажите, я покажу самый маленький new_for_tests().
        AuthVerifier::new_for_tests(RPConfig::default())
    }

    #[test]
    fn decide_auth_redirects_when_no_cookie_header() {
        let v = verifier_minimal();
        let uri: Uri = "http://example.local/".parse().unwrap();

        let d = v.decide_auth(&uri, None);
        assert_eq!(d, AuthDecision::RedirectToSso);
    }

    #[test]
    fn decide_auth_redirects_when_cookie_missing() {
        let v = verifier_minimal();
        let uri: Uri = "http://example.local/".parse().unwrap();

        let d = v.decide_auth(&uri, Some("other=1; something=2"));
        assert_eq!(d, AuthDecision::RedirectToSso);
    }

    #[test]
    fn decide_auth_proceeds_when_cookie_present() {
        let v = verifier_minimal();
        let uri: Uri = "http://example.local/".parse().unwrap();

        let d = v.decide_auth(&uri, Some("rproxy_auth=jwt-here; other=1"));
        assert_eq!(
            d,
            AuthDecision::Proceed {
                jwt: "jwt-here".to_string()
            }
        );
    }
}
