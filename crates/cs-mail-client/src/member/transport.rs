//! Bounded HTTPS transport. Loopback HTTP is allowed only for local development.
use super::Error;
use std::io::Read;

pub trait Transport: Send + Sync {
    /// A successful HTTP status is transport evidence, not a domain outcome.
    /// # Errors
    /// Authentication refusal, no answer, or invalid/bounded response.
    fn request(&self, server: &str, route: &str, body: Option<&[u8]>) -> Result<Vec<u8>, Error>;
    /// OIDC discovery (GET) or code exchange (form POST); never logs response bodies.
    /// # Errors
    /// Failed transport, invalid endpoint or refused authentication.
    fn identity_request(&self, url: &str, form: Option<&[(&str, &str)]>) -> Result<Vec<u8>, Error>;
}

pub struct HttpTransport(reqwest::blocking::Client);
impl HttpTransport {
    /// # Errors
    /// The system cannot initialize the HTTPS client.
    pub fn new() -> Result<Self, Error> {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map(Self)
            .map_err(|_| Error::Unavailable)
    }
}
/// # Errors
/// Rejects credentials, query strings, fragments, and non-loopback plaintext HTTP.
pub fn validate_server(server: &str) -> Result<(), Error> {
    let url = reqwest::Url::parse(server).map_err(|_| Error::InvalidData)?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if (url.scheme() != "https" && !(url.scheme() == "http" && local))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.host_str().is_none()
    {
        return Err(Error::InvalidData);
    }
    Ok(())
}
impl Transport for HttpTransport {
    fn identity_request(&self, url: &str, form: Option<&[(&str, &str)]>) -> Result<Vec<u8>, Error> {
        super::login::secure_url(url)?;
        let request = form.map_or_else(|| self.0.get(url), |form| self.0.post(url).form(form));
        let response = request.send().map_err(|_| Error::Offline)?;
        if !response.status().is_success() {
            return Err(Error::Unauthorized);
        }
        let mut bytes = Vec::new();
        response
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Offline)?;
        if bytes.len() > 1024 * 1024 {
            return Err(Error::InvalidData);
        }
        Ok(bytes)
    }
    fn request(&self, server: &str, route: &str, body: Option<&[u8]>) -> Result<Vec<u8>, Error> {
        validate_server(server)?;
        if !route.starts_with("v1/") || route.contains("..") {
            return Err(Error::InvalidData);
        }
        let url = format!("{}/{route}", server.trim_end_matches('/'));
        let request = body.map_or_else(
            || self.0.get(&url),
            |body| {
                self.0
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .body(body.to_vec())
            },
        );
        let response = request.send().map_err(|_| Error::Offline)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::Unauthorized);
        }
        if response.status() == reqwest::StatusCode::NOT_FOUND && route == "v1/directory/lookup" {
            return Err(Error::AddressUnavailable);
        }
        if !response.status().is_success() {
            return Err(Error::Unavailable);
        }
        let mut bytes = Vec::new();
        response
            .take(super::MAX_CACHE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Offline)?;
        if bytes.len() > super::MAX_CACHE {
            return Err(Error::InvalidData);
        }
        Ok(bytes)
    }
}
