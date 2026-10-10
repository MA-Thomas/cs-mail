//! The service's member API over HTTP.
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub struct Api {
    client: reqwest::blocking::Client,
    base: reqwest::Url,
}

/// Transport failures do not establish the outcome of a journaled operation.
#[derive(Debug)]
pub enum Failure {
    /// The service answered with an error code.
    Response { status: u16, code: String },
    /// No answer arrived: the request may or may not have been received.
    NoAnswer(String),
}
impl core::fmt::Display for Failure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Response { status, code } => write!(f, "service error ({status}): {code}"),
            Self::NoAnswer(e) => write!(f, "no answer from the service: {e}"),
        }
    }
}

impl Api {
    pub fn new(server: &str) -> Result<Self, String> {
        let base = reqwest::Url::parse(server).map_err(|e| format!("invalid server: {e}"))?;
        let loopback = matches!(base.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"));
        if base.scheme() != "https" && !(base.scheme() == "http" && loopback) {
            return Err("server must use HTTPS, or HTTP on a loopback host".into());
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| format!("http client: {e}"))?;
        Ok(Self { client, base })
    }
    fn url(&self, route: &str) -> Result<reqwest::Url, Failure> {
        self.base
            .join(route)
            .map_err(|e| Failure::NoAnswer(e.to_string()))
    }
    fn answer(response: reqwest::blocking::Response) -> Result<Value, Failure> {
        let status = response.status();
        let body: Value = response
            .json()
            .map_err(|e| Failure::NoAnswer(format!("unreadable answer: {e}")))?;
        if status.is_success() {
            Ok(body)
        } else {
            Err(Failure::Response {
                status: status.as_u16(),
                code: body["error"].as_str().unwrap_or("unknown").to_owned(),
            })
        }
    }
    pub fn post<T: Serialize>(&self, route: &str, body: &T) -> Result<Value, Failure> {
        Self::answer(
            self.client
                .post(self.url(route)?)
                .json(body)
                .send()
                .map_err(|e| Failure::NoAnswer(e.to_string()))?,
        )
    }
    pub fn get(&self, route: &str) -> Result<Value, Failure> {
        Self::answer(
            self.client
                .get(self.url(route)?)
                .send()
                .map_err(|e| Failure::NoAnswer(e.to_string()))?,
        )
    }
}

pub fn decode<T: DeserializeOwned>(value: Value, what: &str) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| format!("unexpected {what} from the service: {e}"))
}
