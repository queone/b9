//! Validated synchronous HTTP transport with injectable execution.

use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::io::{self, Read};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Url;
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderName, HeaderValue, LOCATION};

/// An HTTP method supported by the provider transport slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    Get,
    Head,
    Post,
}

/// One duplicate-safe HTTP header.
#[derive(Clone, Eq, PartialEq)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

impl fmt::Display for HttpHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = if is_sensitive_header(&self.name) {
            "<redacted>"
        } else {
            &self.value
        };
        write!(formatter, "{}: {value}", self.name)
    }
}

impl fmt::Debug for HttpHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = if is_sensitive_header(&self.name) {
            "<redacted>"
        } else {
            &self.value
        };
        formatter
            .debug_struct("HttpHeader")
            .field("name", &self.name)
            .field("value", &value)
            .finish()
    }
}

/// One provider-neutral request awaiting validation.
#[derive(Clone, Eq, PartialEq)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<HttpHeader>,
    pub body: Vec<u8>,
    pub timeout: Duration,
    pub body_limit: usize,
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &self.headers)
            .field("body", &"<omitted>")
            .field("timeout", &self.timeout)
            .field("body_limit", &self.body_limit)
            .finish()
    }
}

impl fmt::Display for HttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?} {}", self.method, self.url)?;
        for header in &self.headers {
            write!(formatter, "\n{header}")?;
        }
        write!(formatter, "\n<body omitted>")
    }
}

/// One complete HTTP response with duplicate headers and exact body bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<HttpHeader>,
    pub body: Vec<u8>,
}

/// A validated request that only `HttpClient` can construct.
#[derive(Clone)]
pub struct ValidatedRequest {
    method: HttpMethod,
    url: Url,
    headers: Vec<(HeaderName, HeaderValue)>,
    body: Vec<u8>,
    timeout: Duration,
    body_limit: usize,
}

impl ValidatedRequest {
    /// Return the validated method.
    pub fn method(&self) -> HttpMethod {
        self.method
    }
    /// Return the validated URL.
    pub fn url(&self) -> &str {
        self.url.as_str()
    }
    /// Return duplicate-safe validated headers with textual values.
    pub fn headers(&self) -> Vec<HttpHeader> {
        self.headers
            .iter()
            .map(|(name, value)| HttpHeader {
                name: name.as_str().into(),
                value: value.to_str().unwrap_or("<non-text>").into(),
            })
            .collect()
    }
    /// Return the exact validated request bytes.
    pub fn body(&self) -> &[u8] {
        &self.body
    }
    /// Return the validated total timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
    /// Return the validated response-body limit.
    pub fn body_limit(&self) -> usize {
        self.body_limit
    }
}

impl fmt::Debug for ValidatedRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ValidatedRequest")
            .field("method", &self.method)
            .field("url", &self.url.as_str())
            .field("headers", &self.headers())
            .field("body", &"<omitted>")
            .field("timeout", &self.timeout)
            .field("body_limit", &self.body_limit)
            .finish()
    }
}

/// A validated execution failure.
#[derive(Debug)]
pub enum ExecutorError {
    Dispatch {
        detail: String,
        source: Option<Box<dyn Error + Send + Sync>>,
    },
    Timeout {
        url: String,
    },
    Redirect {
        detail: String,
    },
    ResponseTooLarge {
        limit: usize,
    },
    Read {
        detail: String,
        source: io::Error,
    },
}

impl fmt::Display for ExecutorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dispatch { detail, .. } => write!(
                formatter,
                "dispatch HTTP request: {detail}; check connectivity and TLS configuration, then retry"
            ),
            Self::Timeout { url } => write!(
                formatter,
                "dispatch HTTP request to {url}: total timeout expired; retry when the provider is responsive"
            ),
            Self::Redirect { detail } => write!(
                formatter,
                "follow HTTP redirect: {detail}; verify the provider endpoint and retry"
            ),
            Self::ResponseTooLarge { limit } => write!(
                formatter,
                "read HTTP response: body exceeds {limit} bytes; raise the provider-specific limit only after verifying the response"
            ),
            Self::Read { detail, .. } => write!(
                formatter,
                "read HTTP response: {detail}; retry when the provider is responsive"
            ),
        }
    }
}

impl Error for ExecutorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Dispatch {
                source: Some(source),
                ..
            } => Some(source.as_ref()),
            Self::Read { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Executes requests that have passed `HttpClient` validation.
pub trait HttpExecutor: Send + Sync {
    /// Execute one request already validated by `HttpClient`.
    fn execute(&self, request: ValidatedRequest) -> Result<HttpResponse, ExecutorError>;
}

/// A validated-client failure.
#[derive(Debug)]
pub enum HttpClientError {
    Invalid { detail: String },
    Execute(ExecutorError),
}

impl fmt::Display for HttpClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { detail } => write!(
                formatter,
                "validate HTTP request: {detail}; correct the request and retry"
            ),
            Self::Execute(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for HttpClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Execute(error) => Some(error),
            Self::Invalid { .. } => None,
        }
    }
}

/// Validates provider requests before dispatching through a private executor.
pub struct HttpClient {
    executor: Arc<dyn HttpExecutor>,
}

impl HttpClient {
    /// Construct a client around an injected executor.
    pub fn new(executor: Arc<dyn HttpExecutor>) -> Self {
        Self { executor }
    }

    /// Construct a client using the production blocking HTTPS executor.
    pub fn production() -> Result<Self, HttpClientError> {
        Ok(Self::new(Arc::new(ReqwestExecutor::new()?)))
    }

    /// Validate and execute one request.
    pub fn execute(&self, request: HttpRequest) -> Result<HttpResponse, HttpClientError> {
        let request = validate_request(request)?;
        self.executor
            .execute(request)
            .map_err(HttpClientError::Execute)
    }
}

/// Executes validated requests through reqwest's blocking Rustls client.
pub struct ReqwestExecutor {
    client: Client,
}

impl ReqwestExecutor {
    /// Construct a no-retry, no-automatic-redirect executor.
    pub fn new() -> Result<Self, HttpClientError> {
        Client::builder()
            .tls_backend_rustls()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .map(|client| Self { client })
            .map_err(|error| HttpClientError::Invalid {
                detail: format!("build production HTTP executor: {error}"),
            })
    }
}

impl HttpExecutor for ReqwestExecutor {
    fn execute(&self, request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
        let started = Instant::now();
        let mut current = request;
        let mut visited = HashSet::from([current.url.to_string()]);
        let mut hops = 0usize;
        loop {
            let elapsed = started.elapsed();
            let remaining =
                current
                    .timeout
                    .checked_sub(elapsed)
                    .ok_or_else(|| ExecutorError::Timeout {
                        url: current.url.to_string(),
                    })?;
            let mut builder = match current.method {
                HttpMethod::Get => self.client.get(current.url.clone()),
                HttpMethod::Head => self.client.head(current.url.clone()),
                HttpMethod::Post => self.client.post(current.url.clone()),
            }
            .timeout(remaining);
            for (name, value) in &current.headers {
                builder = builder.header(name, value);
            }
            if matches!(current.method, HttpMethod::Post) {
                builder = builder.body(current.body.clone());
            }
            let response = builder
                .send()
                .map_err(|error| classify_reqwest(error, &current.url))?;
            if matches!(current.method, HttpMethod::Get | HttpMethod::Head)
                && response.status().is_redirection()
            {
                let Some(location) = response.headers().get(LOCATION) else {
                    return read_response(response, current.body_limit);
                };
                if hops == 10 {
                    return Err(ExecutorError::Redirect {
                        detail: "redirect limit of ten was exceeded".into(),
                    });
                }
                let location = location.to_str().map_err(|_| ExecutorError::Redirect {
                    detail: "Location header is not valid text".into(),
                })?;
                let next = current
                    .url
                    .join(location)
                    .map_err(|error| ExecutorError::Redirect {
                        detail: format!("invalid Location header: {error}"),
                    })?;
                validate_redirect_url(&current.url, &next)?;
                if !visited.insert(next.to_string()) {
                    return Err(ExecutorError::Redirect {
                        detail: "redirect loop detected".into(),
                    });
                }
                if origin(&current.url) != origin(&next) {
                    current
                        .headers
                        .retain(|(name, _)| !is_redirect_sensitive(name.as_str()));
                }
                current.url = next;
                hops += 1;
                continue;
            }
            return read_response(response, current.body_limit);
        }
    }
}

fn validate_request(request: HttpRequest) -> Result<ValidatedRequest, HttpClientError> {
    if request.timeout.is_zero() {
        return Err(invalid("timeout must be positive"));
    }
    if request.body_limit == 0 {
        return Err(invalid("body limit must be positive"));
    }
    if !matches!(request.method, HttpMethod::Post) && !request.body.is_empty() {
        return Err(invalid("only POST requests may contain a body"));
    }
    let url =
        Url::parse(&request.url).map_err(|error| invalid(format!("URL is invalid: {error}")))?;
    validate_initial_url(&url)?;
    let mut headers = Vec::with_capacity(request.headers.len());
    for header in request.headers {
        let name = HeaderName::from_bytes(header.name.as_bytes())
            .map_err(|_| invalid("header name is invalid"))?;
        let value = HeaderValue::from_str(&header.value).map_err(|_| {
            invalid(format!(
                "header {:?} contains invalid characters",
                header.name
            ))
        })?;
        headers.push((name, value));
    }
    Ok(ValidatedRequest {
        method: request.method,
        url,
        headers,
        body: request.body,
        timeout: request.timeout,
        body_limit: request.body_limit,
    })
}

fn validate_initial_url(url: &Url) -> Result<(), HttpClientError> {
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("URL credentials are prohibited"));
    }
    if url.fragment().is_some() {
        return Err(invalid("URL fragments are prohibited"));
    }
    match url.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(url) => Ok(()),
        "http" => Err(invalid("HTTP is permitted only for loopback fixtures")),
        _ => Err(invalid("URL scheme must be HTTPS or loopback HTTP")),
    }
}

fn validate_redirect_url(previous: &Url, next: &Url) -> Result<(), ExecutorError> {
    if !next.username().is_empty() || next.password().is_some() || next.fragment().is_some() {
        return Err(ExecutorError::Redirect {
            detail: "redirect target contains credentials or a fragment".into(),
        });
    }
    if previous.scheme() == "https" && next.scheme() == "http" {
        return Err(ExecutorError::Redirect {
            detail: "HTTPS-to-HTTP downgrade is prohibited".into(),
        });
    }
    match next.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(previous) && is_loopback(next) => Ok(()),
        "http" => Err(ExecutorError::Redirect {
            detail: "non-loopback HTTP target is prohibited".into(),
        }),
        _ => Err(ExecutorError::Redirect {
            detail: "unsupported redirect scheme".into(),
        }),
    }
}

fn read_response(mut response: Response, limit: usize) -> Result<HttpResponse, ExecutorError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(ExecutorError::ResponseTooLarge { limit });
    }
    let status = response.status().as_u16();
    let mut headers = Vec::new();
    for name in response.headers().keys() {
        for value in response.headers().get_all(name) {
            headers.push(HttpHeader {
                name: name.as_str().into(),
                value: value.to_str().unwrap_or("<non-text>").into(),
            });
        }
    }
    let mut body =
        Vec::with_capacity(response.content_length().unwrap_or(0).min(limit as u64) as usize);
    response
        .by_ref()
        .take(limit as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|source| ExecutorError::Read {
            detail: "response body read failed".into(),
            source,
        })?;
    if body.len() > limit {
        return Err(ExecutorError::ResponseTooLarge { limit });
    }
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

fn classify_reqwest(error: reqwest::Error, url: &Url) -> ExecutorError {
    if error.is_timeout() {
        ExecutorError::Timeout {
            url: url.to_string(),
        }
    } else {
        ExecutorError::Dispatch {
            detail: error.to_string(),
            source: Some(Box::new(error)),
        }
    }
}

fn invalid(detail: impl Into<String>) -> HttpClientError {
    HttpClientError::Invalid {
        detail: detail.into(),
    }
}
fn is_loopback(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    })
}
fn origin(url: &Url) -> (&str, Option<&str>, Option<u16>) {
    (url.scheme(), url.host_str(), url.port_or_known_default())
}
fn is_redirect_sensitive(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization" | "proxy-authorization" | "cookie" | "cookie2"
    )
}
fn is_sensitive_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    is_redirect_sensitive(&lower)
        || lower.contains("token")
        || lower.contains("api-key")
        || lower.contains("apikey")
        || lower.contains("secret")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_validation_and_redaction_are_closed() {
        let secure = Url::parse("https://example.test/start").unwrap();
        let downgrade = Url::parse("http://example.test/end").unwrap();
        assert!(validate_redirect_url(&secure, &downgrade).is_err());
        let header = HttpHeader {
            name: "X-Api-Key".into(),
            value: "private".into(),
        };
        assert!(!format!("{header:?}").contains("private"));
    }
}
