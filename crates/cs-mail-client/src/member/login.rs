//! Native authorization-code flow. The identity service verifies ID-token signatures and
//! owns identity decisions; these checks bind the browser response to this device ceremony.
use super::{Error, Transport};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub const CALLBACK: &str = "com.csqd.member.development:/oauth/callback";
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct LoginConfig {
    pub issuer: String,
    pub client_id: String,
    pub redirect_uri: String,
}
#[derive(Clone)]
pub struct Authorization {
    pub url: String,
    pub callback_scheme: String,
}
pub(super) struct Attempt {
    config: LoginConfig,
    token_endpoint: String,
    state: String,
    verifier: Zeroizing<String>,
    pub intent: identity_contract::EnrollmentIntent,
    pub details: super::EnrollmentDetails,
}
#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    code_challenge_methods_supported: Vec<String>,
}

pub(super) fn secure_url(value: &str) -> Result<reqwest::Url, Error> {
    let url = reqwest::Url::parse(value).map_err(|_| Error::InvalidData)?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if (url.scheme() != "https" && !(url.scheme() == "http" && local))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::InvalidData);
    }
    Ok(url)
}
fn random() -> Result<String, Error> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| Error::Unavailable)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
impl Attempt {
    pub fn begin(
        config: LoginConfig,
        intent: identity_contract::EnrollmentIntent,
        details: super::EnrollmentDetails,
        transport: &dyn Transport,
    ) -> Result<(Self, Authorization), Error> {
        let issuer = secure_url(&config.issuer)?;
        if issuer.query().is_some()
            || config.redirect_uri != CALLBACK
            || config.client_id.is_empty()
        {
            return Err(Error::InvalidData);
        }
        let bytes = transport.identity_request(
            &format!(
                "{}/.well-known/openid-configuration",
                config.issuer.trim_end_matches('/')
            ),
            None,
        )?;
        let discovery: Discovery =
            serde_json::from_slice(&bytes).map_err(|_| Error::InvalidData)?;
        if discovery.issuer != config.issuer
            || !discovery
                .code_challenge_methods_supported
                .iter()
                .any(|v| v == "S256")
        {
            return Err(Error::InvalidData);
        }
        let mut url = secure_url(&discovery.authorization_endpoint)?;
        let token = secure_url(&discovery.token_endpoint)?;
        // A production issuer cannot downgrade discovery to a plaintext endpoint.
        if issuer.scheme() == "https" && (url.scheme() != "https" || token.scheme() != "https") {
            return Err(Error::InvalidData);
        }
        if url.query().is_some() || token.query().is_some() {
            return Err(Error::InvalidData);
        }
        let state = random()?;
        let verifier = Zeroizing::new(random()?);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        url.query_pairs_mut().extend_pairs([
            ("response_type", "code"),
            ("client_id", &config.client_id),
            ("redirect_uri", &config.redirect_uri),
            ("scope", "openid"),
            ("nonce", &intent.challenge),
            ("state", &state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("max_age", "0"),
            ("prompt", "login"),
        ]);
        let authorization = Authorization {
            url: url.into(),
            callback_scheme: "com.csqd.member.development".into(),
        };
        Ok((
            Self {
                config,
                token_endpoint: discovery.token_endpoint,
                state,
                verifier,
                intent,
                details,
            },
            authorization,
        ))
    }
    pub fn exchange(
        &self,
        callback: &str,
        now: i64,
        transport: &dyn Transport,
    ) -> Result<(Zeroizing<String>, String, String), Error> {
        self.intent.validate(now).map_err(|_| Error::Unauthorized)?;
        let mut url = reqwest::Url::parse(callback).map_err(|_| Error::InvalidData)?;
        let pairs: Vec<_> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        url.set_query(None);
        if url.as_str() != self.config.redirect_uri {
            return Err(Error::Unauthorized);
        }
        let exactly_one = |key: &str| -> Result<&str, Error> {
            let mut values = pairs
                .iter()
                .filter(|(k, _)| k == key)
                .map(|(_, v)| v.as_str());
            let value = values.next().ok_or(Error::Unauthorized)?;
            if values.next().is_some() || value.is_empty() {
                return Err(Error::Unauthorized);
            }
            Ok(value)
        };
        if exactly_one("state")? != self.state {
            return Err(Error::Unauthorized);
        }
        if pairs.iter().any(|(k, _)| k == "error") {
            return Err(Error::LoginCancelled);
        }
        if pairs.iter().any(|(k, _)| k == "iss") && exactly_one("iss")? != self.config.issuer {
            return Err(Error::Unauthorized);
        }
        let form = [
            ("grant_type", "authorization_code"),
            ("code", exactly_one("code")?),
            ("client_id", &self.config.client_id),
            ("redirect_uri", &self.config.redirect_uri),
            ("code_verifier", self.verifier.as_str()),
        ];
        let bytes = Zeroizing::new(transport.identity_request(&self.token_endpoint, Some(&form))?);
        let token = Zeroizing::new(
            serde_json::from_slice::<Tokens>(&bytes)
                .map_err(|_| Error::Unauthorized)?
                .id_token,
        );
        let parts: Vec<_> = token.split('.').collect();
        if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
            return Err(Error::Unauthorized);
        }
        let payload = Zeroizing::new(
            URL_SAFE_NO_PAD
                .decode(parts[1])
                .map_err(|_| Error::Unauthorized)?,
        );
        let claims: Claims = serde_json::from_slice(&payload).map_err(|_| Error::Unauthorized)?;
        let audience = claims.aud.as_str() == Some(&self.config.client_id)
            || claims
                .aud
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(&self.config.client_id)));
        if claims.iss != self.config.issuer
            || claims.sub.is_empty()
            || claims.nonce != self.intent.challenge
            || !audience
            || claims.exp <= now
        {
            return Err(Error::Unauthorized);
        }
        // Routing claims only: no account is activated before the server verifies the token.
        Ok((token, claims.iss, claims.sub))
    }
}

#[derive(Deserialize)]
struct Tokens {
    id_token: String,
}
#[derive(Deserialize)]
struct Claims {
    iss: String,
    sub: String,
    nonce: String,
    aud: serde_json::Value,
    exp: i64,
}
