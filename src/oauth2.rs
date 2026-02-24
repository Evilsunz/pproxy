use std::fs;
use anyhow::anyhow;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use oauth2::{
    AuthorizationCode,
    AuthUrl,
    ClientId,
    ClientSecret,
    CsrfToken,
    PkceCodeChallenge,
    RedirectUrl,
    Scope,
    TokenResponse,
    TokenUrl
};
use oauth2::basic::BasicClient;
use oauth2::reqwest;
use crate::lb::{AuthClaims, AuthVerifier};

fn main() {
    let sub = "max@comcast.com";
    let tid = "rproxy";
    let iss = "rproxy";
    let aud = "rproxy";

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let verifier = AuthVerifier::new(&fs::read("./config/jwt.pem").unwrap(), &fs::read("./config/jwt_private.pem").unwrap(),iss,aud).unwrap();
    let claims  = AuthClaims {
        sub: sub.to_string(),
        tid: tid.to_string(),
        exp: now,
        iat: now + (60 * 60 * 6),
        iss: iss.to_string(),
        aud: aud.to_string(),
    };
    let rez = encode(&Header::new(Algorithm::RS256), &claims, &verifier.encoding_key).unwrap();
    println!("{}", rez);
    let data = decode::<AuthClaims>(&rez, &verifier.decoding_key, &verifier.validation).unwrap().claims;
    println!("{:?}", data);
}

impl AuthVerifier {
    pub fn new(public_pem: &[u8],private_pem: &[u8], iss: &str, aud: &str) -> anyhow::Result<Self> {
        let decoding_key = DecodingKey::from_rsa_pem(public_pem)?;
        let encoding_key= EncodingKey::from_rsa_pem(private_pem)?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[iss]);
        validation.set_audience(&[aud]);

        Ok(Self { decoding_key, encoding_key, validation })
    }
}

fn get_cookie_value(cookie_header: &str, name: &str) -> Option<String> {
    for part in cookie_header.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}

fn verify_auth_cookie(verifier: &AuthVerifier, cookie_header: &str) -> anyhow::Result<AuthClaims> {
    let jwt = get_cookie_value(cookie_header, "consul_ui_auth")
        .ok_or_else(|| anyhow!("auth cookie missing"))?;
    let header = Header::new(Algorithm::RS256);
    let data = decode::<AuthClaims>(&jwt, &verifier.decoding_key, &verifier.validation)?;
    Ok(data.claims)
}

// #[tokio::main]
// async fn main()-> anyhow::Result<()>{
//     // Create an OAuth2 client by specifying the client ID, client secret, authorization URL and
//     // token URL.
//     let client = BasicClient::new(ClientId::new("client_id".to_string()))
//         .set_client_secret(ClientSecret::new("client_secret".to_string()))
//         .set_auth_uri(AuthUrl::new("http://authorize".to_string())?)
//         .set_token_uri(TokenUrl::new("http://token".to_string())?)
//         // Set the URL the user will be redirected to after the authorization process.
//         .set_redirect_uri(RedirectUrl::new("http://redirect".to_string())?);
//
//     // Generate a PKCE challenge.
//     let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
//
//     // Generate the full authorization URL.
//     let (auth_url, csrf_token) = client
//         .authorize_url(CsrfToken::new_random)
//         // Set the desired scopes.
//         .add_scope(Scope::new("read".to_string()))
//         .add_scope(Scope::new("write".to_string()))
//         // Set the PKCE code challenge.
//         .set_pkce_challenge(pkce_challenge)
//         .url();
//
//     // This is the URL you should redirect the user to, in order to trigger the authorization
//     // process.
//     println!("Browse to: {}", auth_url);
//
//     // Once the user has been redirected to the redirect URL, you'll have access to the
//     // authorization code. For security reasons, your code should verify that the `state`
//     // parameter returned by the server matches `csrf_token`.
//
//     let http_client = reqwest::ClientBuilder::new()
//         // Following redirects opens the client up to SSRF vulnerabilities.
//         .redirect(reqwest::redirect::Policy::none())
//         .build()
//         .expect("Client should build");
//
//     // Now you can trade it for an access token.
//     let token_result = client
//         .exchange_code(AuthorizationCode::new("some authorization code".to_string()))
//         // Set the PKCE code verifier.
//         .set_pkce_verifier(pkce_verifier)
//         .request_async(&http_client)
//         .await?;
//     Ok(())
// }

