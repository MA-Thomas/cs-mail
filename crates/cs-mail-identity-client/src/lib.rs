//! HTTP transport for the versioned shared identity contract.
use identity_contract::{ClientError, Error};
use std::io::Read;
/// Transport configuration is deployment-owned. Production URLs require HTTPS.
pub struct HttpIdentityClient {
    client: reqwest::blocking::Client,
    endpoint: reqwest::Url,
}
impl HttpIdentityClient {
    /// # Errors
    /// Rejects non-HTTPS endpoints except explicit loopback development hosts.
    pub fn new(endpoint: &str) -> Result<Self, ClientError> {
        let endpoint = reqwest::Url::parse(endpoint)
            .map_err(|e| ClientError::with_source(Error::Invalid, e))?;
        let local = matches!(
            endpoint.host_str(),
            Some("127.0.0.1" | "[::1]" | "localhost")
        );
        if endpoint.scheme() != "https" && !(endpoint.scheme() == "http" && local) {
            return Err(Error::Invalid.into());
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| ClientError::with_source(Error::Unavailable, e))?;
        Ok(Self { client, endpoint })
    }
}
impl identity_contract::IdentityClient for HttpIdentityClient {
    fn call(
        &self,
        request: &identity_contract::SignedRequest,
    ) -> Result<identity_contract::Response, ClientError> {
        let response = self
            .client
            .post(self.endpoint.clone())
            .json(request)
            .send()
            .map_err(|e| ClientError::with_source(Error::Unavailable, e))?;
        if !response.status().is_success() {
            return Err(ClientError::with_source(
                Error::Unavailable,
                std::io::Error::other(format!("identity HTTP status {}", response.status())),
            ));
        }
        // Bound response allocation independently of an untrusted Content-Length.
        let mut data = Vec::new();
        response
            .take(65_537)
            .read_to_end(&mut data)
            .map_err(|e| ClientError::with_source(Error::Unavailable, e))?;
        if data.len() > 65_536 {
            return Err(Error::Invalid.into());
        }
        serde_json::from_slice(&data).map_err(|e| ClientError::with_source(Error::Invalid, e))
    }
}
