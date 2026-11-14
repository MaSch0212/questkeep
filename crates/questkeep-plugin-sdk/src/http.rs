//! HTTP through the app. Only hosts declared under `permissions.http` in `plugin.json` can be
//! reached, over https (plain http only for hosts declared as `http://host`).

use crate::ProviderError;

/// A request to send with [`request`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

impl Request {
    pub fn new(method: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            url: url.into(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Some(body.into());
        self
    }
}

/// The server's answer, whatever its status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// 2xx.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The body as text (invalid UTF-8 is replaced).
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The body parsed as JSON.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, ProviderError> {
        serde_json::from_slice(&self.body).map_err(|e| ProviderError::Parse(e.to_string()))
    }

    /// The first header with this name (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// `GET url`.
pub fn get(url: impl Into<String>) -> Result<Response, ProviderError> {
    request(Request::new("GET", url))
}

/// Sends the request. An answer with an error status is still `Ok`; `Err` (a
/// [`ProviderError::Network`]) means the host is not declared or the request failed.
pub fn request(request: Request) -> Result<Response, ProviderError> {
    send(request).map_err(ProviderError::Network)
}

#[cfg(target_arch = "wasm32")]
fn send(request: Request) -> Result<Response, String> {
    use crate::bindings::questkeep::plugin::host;
    let response = host::http_request(&host::Request {
        method: request.method,
        url: request.url,
        headers: request.headers,
        body: request.body,
    })?;
    Ok(Response {
        status: response.status,
        headers: response.headers,
        body: response.body,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn send(_request: Request) -> Result<Response, String> {
    Err("HTTP requests only work inside Questkeep".into())
}
