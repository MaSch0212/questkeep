//! A tiny HTTP abstraction so sign-in code can be tested without a network, and so request
//! secrets (codes, tokens, client secrets) never end up in error messages or logs.

use std::time::Duration;

/// Request method; sign-in only needs these two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    PostForm,
}

/// A request. `Debug` deliberately omits the query string, headers and form body because they
/// carry secrets.
#[derive(Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub form: Vec<(String, String)>,
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: Method::Get,
            url: url.into(),
            headers: Vec::new(),
            form: Vec::new(),
        }
    }

    pub fn post_form(url: impl Into<String>, form: Vec<(String, String)>) -> Self {
        Self {
            method: Method::PostForm,
            url: url.into(),
            headers: Vec::new(),
            form,
        }
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    pub fn bearer(self, token: &str) -> Self {
        self.header("Authorization", format!("Bearer {token}"))
    }

    /// Every value in this request that must not be shown.
    fn secrets(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (_, value) in &self.headers {
            out.push(value.clone());
            if let Some((_, rest)) = value.split_once(' ') {
                out.push(rest.to_string());
            }
        }
        out.extend(self.form.iter().map(|(_, v)| v.clone()));
        if let Some((_, query)) = self.url.split_once('?') {
            for pair in query.split('&') {
                if let Some((_, v)) = pair.split_once('=') {
                    out.push(v.to_string());
                    out.push(super::url_decode(v));
                }
            }
        }
        out
    }
}

impl std::fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let url = self.url.split('?').next().unwrap_or_default();
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &url)
            .finish_non_exhaustive()
    }
}

/// A response with any status code (the caller decides what is an error).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Sends requests. Transport failures are returned as already-redacted text.
pub trait Http: Send + Sync {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String>;
}

/// Replaces every secret of `request` found in `message`. Short values are left alone so that
/// `true` or `en-US` do not mangle the text.
pub fn redact_secrets(message: &str, request: &HttpRequest) -> String {
    let mut out = message.to_string();
    for secret in request.secrets() {
        if secret.len() >= 6 {
            out = out.replace(&secret, "<redacted>");
        }
    }
    out
}

/// The real implementation, on `ureq`.
pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .user_agent(concat!("Questkeep/", env!("CARGO_PKG_VERSION")))
            // Status codes are data here: a 400 from a token endpoint means "bad code".
            .http_status_as_error(false)
            .build()
            .into();
        Self { agent }
    }
}

impl Default for UreqHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl Http for UreqHttp {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        let fail = |e: String| redact_secrets(&e, request);
        let result = match request.method {
            Method::Get => {
                let mut builder = self.agent.get(&request.url);
                for (name, value) in &request.headers {
                    builder = builder.header(name, value);
                }
                builder.call()
            }
            Method::PostForm => {
                let body = request
                    .form
                    .iter()
                    .map(|(k, v)| format!("{}={}", super::url_encode(k), super::url_encode(v)))
                    .collect::<Vec<_>>()
                    .join("&");
                let mut builder = self
                    .agent
                    .post(&request.url)
                    .header("Content-Type", "application/x-www-form-urlencoded");
                for (name, value) in &request.headers {
                    builder = builder.header(name, value);
                }
                builder.send(body)
            }
        };
        let mut response = result.map_err(|e| fail(e.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| fail(e.to_string()))?;
        Ok(HttpResponse { status, body })
    }
}

/// Scripted HTTP for tests: a handler plus a log of what was sent.
#[cfg(test)]
pub(crate) mod testing {
    use std::sync::Mutex;

    use super::*;

    type Handler = dyn Fn(&HttpRequest) -> Result<HttpResponse, String> + Send + Sync;

    pub struct FakeHttp {
        handler: Box<Handler>,
        pub log: Mutex<Vec<HttpRequest>>,
    }

    impl FakeHttp {
        pub fn new(
            handler: impl Fn(&HttpRequest) -> Result<HttpResponse, String> + Send + Sync + 'static,
        ) -> Self {
            Self {
                handler: Box::new(handler),
                log: Mutex::new(Vec::new()),
            }
        }

        pub fn urls(&self) -> Vec<String> {
            self.log
                .lock()
                .map(|l| l.iter().map(|r| r.url.clone()).collect())
                .unwrap_or_default()
        }
    }

    impl Http for FakeHttp {
        fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
            if let Ok(mut log) = self.log.lock() {
                log.push(request.clone());
            }
            (self.handler)(request)
        }
    }

    pub fn response(status: u16, body: &str) -> Result<HttpResponse, String> {
        Ok(HttpResponse {
            status,
            body: body.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_query_headers_and_form() {
        let request = HttpRequest::post_form(
            "https://x.test/token?code=SECRETCODE1",
            vec![("refresh_token".into(), "SECRETREFRESH".into())],
        )
        .bearer("SECRETBEARER");
        let text = format!("{request:?}");
        assert!(text.contains("https://x.test/token"));
        for secret in ["SECRETCODE1", "SECRETREFRESH", "SECRETBEARER"] {
            assert!(!text.contains(secret), "{text}");
        }
    }

    #[test]
    fn redacts_every_secret_of_a_request() {
        let request = HttpRequest::get("https://x.test/t?code=abc%2Fdef123&client_id=1")
            .header("Authorization", "Bearer TOKENVALUE");
        let message =
            "error sending https://x.test/t?code=abc%2Fdef123 with TOKENVALUE and abc/def123, id 1";
        let clean = redact_secrets(message, &request);
        assert!(!clean.contains("TOKENVALUE"));
        assert!(!clean.contains("abc%2Fdef123"));
        assert!(!clean.contains("abc/def123"));
        assert!(clean.contains("id 1"), "short values are kept: {clean}");
    }
}
