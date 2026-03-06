#[cfg(test)]
mod tests;

use crate::config::RPConfig;
use crate::structs::{AuthClaims, AuthDecision, AuthVerifier};
use crate::{log_error, log_trace};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use oauth2::basic::BasicClient;
use oauth2::http::Uri;
use oauth2::url::Url;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
use pingora::ErrorType;
use pingora::http::{ResponseHeader, StatusCode};
use pingora::prelude::Session;
use serde_json::Value;
use std::fs;

const COOKIE_NAME: &str = "rproxy_auth";
const ISSUER: &str = "rproxy";
const COOKIE_HEADER_NAME: &str = "Cookie";

impl AuthVerifier {
    pub fn new(rp_config: RPConfig) -> Self {
        let jwt_pub_pem = fs::read(&rp_config.jwt_cert).unwrap_or_else(|e| {
            panic!(
                "Failed to read jwt_cert PEM file '{}': {e}",
                &rp_config.jwt_cert
            )
        });

        let jwt_priv_pem = fs::read(&rp_config.jwt_private_cert).unwrap_or_else(|e| {
            panic!(
                "Failed to read jwt_private_cert PEM file '{}': {e}",
                &rp_config.jwt_private_cert
            )
        });

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

        let client = BasicClient::new(ClientId::new(rp_config.client_id.clone()))
            .set_client_secret(ClientSecret::new(rp_config.client_secret.clone()))
            .set_auth_uri(AuthUrl::new(rp_config.auth_url.clone()).expect("Invalid auth url"))
            .set_token_uri(TokenUrl::new(rp_config.token_url.clone()).expect("Invalid token url"));
            // .set_redirect_uri(
            //     RedirectUrl::new(rp_config.redirect_url.clone()).expect("Invalid redirect url"),
            // );

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

    pub async fn verify_auth_cookie(&self, session: &mut Session, redirect_url: String) -> pingora::Result<bool> {
        log_trace!("Uri host{}", session.req_header().uri);

        let cookie_header = session
            .get_header(COOKIE_HEADER_NAME)
            .and_then(|h| h.to_str().ok());

        match self.decide_auth(&session.req_header().uri, cookie_header) {
            AuthDecision::Exchange { code } => self.exchange(&code, session).await,
            AuthDecision::RedirectToSso => self.redirect_to_sso(session, redirect_url).await,
            AuthDecision::Proceed => Ok(false),
        }
    }

    fn decide_auth(&self, uri: &Uri, cookie_header: Option<&str>) -> AuthDecision {
        if let Some(code) = self.is_oauth_redirect_with_code(uri) {
            return AuthDecision::Exchange { code };
        }

        let Some(cookie_header) = cookie_header else {
            return AuthDecision::RedirectToSso;
        };

        let Some(jwt) = self.is_have_cookie_value_by_name(cookie_header, COOKIE_NAME) else {
            return AuthDecision::RedirectToSso;
        };

        if self.decode_jwt(&jwt).is_err() {
            return AuthDecision::RedirectToSso;
        }

        AuthDecision::Proceed
    }

    async fn redirect_to_sso(&self, session: &mut Session, redirect_url: String) -> pingora::Result<bool> {
        log_trace!(
            "Redirecting to SSO + req summary {}",
            session.request_summary()
        );

        let location = match self.get_redirect_url(redirect_url) {
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

    fn is_oauth_redirect_with_code(&self, uri: &Uri) -> Option<String> {
        const HOST_PREFIX: &str = "http://localhost/";
        let url = &(HOST_PREFIX.to_string() + &uri.to_string());
        let Ok(parsed_url) = Url::parse(url) else {
            log_error!("Failed to parse URL: {}", url);
            return None;
        };
        parsed_url
            .query_pairs()
            .find(|(key, _)| key == "code")
            .map(|(_, code)| AuthorizationCode::new(code.into_owned()))
            .map(|code| code.secret().to_owned())
    }

    async fn exchange(&self, code: &str, session: &mut Session) -> pingora::Result<bool> {
        let token = match self.client.exchange_code(AuthorizationCode::new(code.to_string())).request_async(&self.http_client).await {
            Ok(t) => {t}
            Err(_) => {
                return Err(pingora::Error::new(ErrorType::HTTPStatus(401)))
            }
        };

        let jwt = token.access_token().secret();
        let claims = self.decode_jwt_unverified(jwt).await?;
        let name = claims
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("name_unknown");
        let tid = claims
            .get("tid")
            .and_then(Value::as_str)
            .unwrap_or("tid_unknown");

        let jwt = self.encode_jwt(name, tid).unwrap();

        const SECS_PER_DAY: u64 = 24 * 60 * 60;
        let mut resp = ResponseHeader::build(StatusCode::SEE_OTHER, Some(0))?;
        let days = u64::from(self.rp_config.sso_cookie_expire_dayz);
        let max_age = SECS_PER_DAY.saturating_mul(days);
        let cookie_value = format!(
            "{name}={val}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={age}",
            name = COOKIE_NAME,
            val = jwt,
            age = max_age
        );
        resp.insert_header("Set-Cookie", cookie_value)?;
        resp.insert_header("Location", "/")?;
        session.write_response_header(Box::new(resp), true).await?;

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
            .as_secs();

        let claims = AuthClaims {
            sub: sub.to_string(),
            tid: tid.to_string(),
            exp: now + (60 * 60 * 24 * u64::from(self.rp_config.sso_cookie_expire_dayz)),
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

    fn get_redirect_url(&self, redirect_url : String) -> anyhow::Result<String> {
        let (auth_url, _) = self
            .client
            .clone()
            .set_redirect_uri(RedirectUrl::new(redirect_url).expect("Invalid redirect url"))
            .authorize_url(CsrfToken::new_random)
            .add_scopes(
                self.rp_config
                    .scopes
                    .iter()
                    .map(|s| Scope::new(s.to_string())),
            )
            .url();

        Ok(auth_url.to_string())
    }

    async fn decode_jwt_unverified(&self, jwt: &str) -> Result<Value, pingora::Error> {
        let parts: Vec<&str> = jwt.split('.').collect();
        if parts.len() != 3 {
            return Err(*pingora::Error::new(ErrorType::HTTPStatus(401)));
        }
        let claims = URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|_| *pingora::Error::new(ErrorType::HTTPStatus(401)))?;
        serde_json::from_slice::<Value>(&claims)
            .map_err(|_| *pingora::Error::new(ErrorType::HTTPStatus(401)))
    }
}
