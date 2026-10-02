//! Shared provider HTTP plumbing: fixed HTTPS origin, no redirects, no proxies,
//! bounded time and body size, sensitive auth headers, sanitized errors (R-SEC).

use std::time::Duration;

use gbm_application::providers::ProviderError;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, StatusCode, Url, redirect};

pub(crate) const USER_AGENT: &str = concat!("git-backup-manager/", env!("CARGO_PKG_VERSION"));
const MAX_BODY_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct Limits {
    pub timeout: Duration,
    pub connect_timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
            connect_timeout: Duration::from_secs(10),
        }
    }
}

/// A client bound to one origin. Production origins are HTTPS-only; tests may use a
/// loopback HTTP origin through `ProviderHttp::for_test`.
pub(crate) struct ProviderHttp {
    client: Client,
    origin: Url,
}

pub(crate) struct HttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl ProviderHttp {
    pub(crate) fn new(origin: &str, limits: &Limits) -> Result<Self, ProviderError> {
        Self::build(origin, limits, true)
    }

    #[cfg(test)]
    pub(crate) fn for_test(origin: &str, limits: &Limits) -> Result<Self, ProviderError> {
        Self::build(origin, limits, false)
    }

    fn build(origin: &str, limits: &Limits, https_only: bool) -> Result<Self, ProviderError> {
        let origin = Url::parse(origin).map_err(|_| ProviderError::UnexpectedResponse)?;
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .redirect(redirect::Policy::none())
            .no_proxy()
            .https_only(https_only)
            .timeout(limits.timeout)
            .connect_timeout(limits.connect_timeout)
            .build()
            .map_err(|_| ProviderError::Network)?;
        Ok(Self { client, origin })
    }

    /// GET `path` on the bound origin with one auth header. The header value is marked
    /// sensitive so it is omitted from reqwest's Debug output.
    pub(crate) async fn get(
        &self,
        path: &str,
        auth: (HeaderName, &str),
        extra: &[(HeaderName, HeaderValue)],
    ) -> Result<HttpResponse, ProviderError> {
        let url = self
            .origin
            .join(path)
            .map_err(|_| ProviderError::UnexpectedResponse)?;
        if url.origin() != self.origin.origin() {
            return Err(ProviderError::UnexpectedResponse);
        }
        let mut value = HeaderValue::from_str(auth.1).map_err(|_| ProviderError::MalformedToken)?;
        value.set_sensitive(true);
        let mut request = self.client.get(url).header(auth.0, value);
        for (name, value) in extra {
            request = request.header(name.clone(), value.clone());
        }
        let mut response = request.send().await.map_err(map_reqwest)?;
        let status = response.status();
        let headers = response.headers().clone();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(map_reqwest)? {
            if body.len() + chunk.len() > MAX_BODY_BYTES {
                return Err(ProviderError::UnexpectedResponse);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

/// Never formats the reqwest error (it can include URLs and backend text).
fn map_reqwest(error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        ProviderError::Timeout
    } else if error.is_redirect() || error.is_decode() || error.is_body() {
        ProviderError::UnexpectedResponse
    } else {
        ProviderError::Network
    }
}

pub(crate) fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

pub(crate) fn retry_after(headers: &HeaderMap) -> Option<u64> {
    header_str(headers, "retry-after").and_then(|v| v.trim().parse().ok())
}

/// Status classes shared by both providers (provider-specific cases first).
pub(crate) fn classify_failure(response: &HttpResponse) -> ProviderError {
    match response.status.as_u16() {
        401 => ProviderError::InvalidOrExpired,
        429 => ProviderError::RateLimited {
            retry_after_seconds: retry_after(&response.headers),
        },
        403 => ProviderError::AccessDenied,
        500..=599 => ProviderError::ProviderUnavailable,
        _ => ProviderError::UnexpectedResponse,
    }
}

#[cfg(test)]
pub(crate) mod test_server;
