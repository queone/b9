//! Yahoo OAuth, secure token handling, and authenticated raw acquisition.

use std::env;
use std::fmt;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::transport::{HttpClient, HttpHeader, HttpMethod, HttpRequest, HttpResponse};

const KEYRING_SERVICE: &str = "b9";
const KEYRING_ACCOUNT: &str = "yahoo-oauth-token";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const TOKEN_BODY_LIMIT: usize = 64 * 1024;
const RAW_BODY_LIMIT: usize = 8 * 1024 * 1024;
const EXPIRY_MARGIN: Duration = Duration::from_secs(10);
const MAX_ATTEMPTS: usize = 5;

/// One Yahoo adapter failure with secret-safe context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum YahooError {
    Configuration(&'static str),
    Authorization(&'static str),
    Credential(&'static str),
    NotAuthenticated,
    SessionExpired,
    TerminalAccess { status: u16 },
    RateLimited,
    InvalidPath(&'static str),
    Request(&'static str),
    TokenResponse(&'static str),
}

impl YahooError {
    /// Report whether this failure must terminate the current Yahoo operation cycle.
    pub fn is_terminal_access(&self) -> bool {
        matches!(self, Self::TerminalAccess { .. })
    }
}

impl fmt::Display for YahooError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(detail) => write!(
                formatter,
                "configure Yahoo access: {detail}; correct the Yahoo configuration and retry"
            ),
            Self::Authorization(detail) => write!(
                formatter,
                "complete Yahoo authorization: {detail}; restart b9 login and retry"
            ),
            Self::Credential(detail) => write!(
                formatter,
                "access Yahoo credential: {detail}; unlock or configure the operating-system credential store and retry"
            ),
            Self::NotAuthenticated => write!(formatter, "not authenticated — run: b9 login"),
            Self::SessionExpired => write!(formatter, "session expired — run: b9 login"),
            Self::TerminalAccess { status } => write!(
                formatter,
                "Yahoo API returned HTTP {status}; run b9 login and retry"
            ),
            Self::RateLimited => write!(
                formatter,
                "Yahoo API remained rate limited after four retries; retry later"
            ),
            Self::InvalidPath(detail) => write!(
                formatter,
                "construct Yahoo API request: {detail}; use a provider-relative path and retry"
            ),
            Self::Request(detail) => write!(
                formatter,
                "request Yahoo API: {detail}; verify connectivity and retry"
            ),
            Self::TokenResponse(detail) => write!(
                formatter,
                "exchange Yahoo token: {detail}; restart b9 login and retry"
            ),
        }
    }
}

impl std::error::Error for YahooError {}

/// One non-fatal Yahoo acquisition issue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum YahooIssue {
    CredentialPersistence,
}

/// One bounded authenticated Yahoo response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YahooRawResponse {
    pub body: Vec<u8>,
    pub issues: Vec<YahooIssue>,
}

/// Public, non-secret status for the stored Yahoo token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YahooTokenStatus {
    pub valid: bool,
    pub has_refresh: bool,
    pub expires_at: Option<SystemTime>,
}

/// Validated Yahoo endpoint configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YahooEndpoints {
    auth: Url,
    token: Url,
    api: Url,
    redirect: Url,
}

impl YahooEndpoints {
    /// Construct validated Yahoo endpoint URLs.
    pub fn new(auth: &str, token: &str, api: &str, redirect: &str) -> Result<Self, YahooError> {
        Ok(Self {
            auth: validate_endpoint("authorization endpoint is invalid", auth)?,
            token: validate_endpoint("token endpoint is invalid", token)?,
            api: validate_endpoint("API endpoint is invalid", api)?,
            redirect: validate_redirect(redirect)?,
        })
    }

    /// Return Yahoo's production endpoints.
    pub fn production() -> Self {
        Self::new(
            "https://api.login.yahoo.com/oauth2/request_auth",
            "https://api.login.yahoo.com/oauth2/get_token",
            "https://fantasysports.yahooapis.com/fantasy/v2",
            "https://localhost:8080/callback",
        )
        .expect("static Yahoo endpoints are valid")
    }
}

/// An opaque one-use authorization session.
pub struct PendingAuthorization {
    state: String,
    verifier: String,
}

impl fmt::Debug for PendingAuthorization {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingAuthorization")
            .field("state", &"<redacted>")
            .field("verifier", &"<redacted>")
            .finish()
    }
}

/// A browser URL paired with its opaque authorization session.
pub struct AuthorizationStart {
    pub url: String,
    pub pending: PendingAuthorization,
}

impl fmt::Debug for AuthorizationStart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthorizationStart")
            .field("url", &"<redacted>")
            .field("pending", &self.pending)
            .finish()
    }
}

/// Stores one serialized Yahoo credential outside repository-owned storage.
pub trait YahooCredentialStore: Send + Sync {
    /// Load the serialized credential, returning `None` only when it is absent.
    fn load(&self) -> Result<Option<String>, YahooError>;
    /// Replace the serialized credential.
    fn save(&self, credential: &str) -> Result<(), YahooError>;
    /// Delete the credential, succeeding when it is already absent.
    fn delete(&self) -> Result<(), YahooError>;
}

/// Supplies deterministic time to Yahoo token handling.
pub trait YahooClock: Send + Sync {
    /// Return the current wall-clock time.
    fn now(&self) -> SystemTime;
}

/// Supplies cryptographically random authorization material.
pub trait YahooNonceSource: Send + Sync {
    /// Return `length` random bytes.
    fn bytes(&self, length: usize) -> Result<Vec<u8>, YahooError>;
}

/// Waits between bounded Yahoo rate-limit retries.
pub trait YahooWaiter: Send + Sync {
    /// Wait for the requested duration.
    fn wait(&self, duration: Duration);
}

/// Production wall clock.
pub struct SystemYahooClock;

impl YahooClock for SystemYahooClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Production operating-system randomness.
pub struct SystemYahooNonceSource;

impl YahooNonceSource for SystemYahooNonceSource {
    fn bytes(&self, length: usize) -> Result<Vec<u8>, YahooError> {
        let mut value = vec![0; length];
        getrandom::fill(&mut value)
            .map_err(|_| YahooError::Authorization("generate secure authorization state"))?;
        Ok(value)
    }
}

/// Production blocking wait boundary.
pub struct ThreadYahooWaiter;

impl YahooWaiter for ThreadYahooWaiter {
    fn wait(&self, duration: Duration) {
        thread::sleep(duration);
    }
}

/// Production operating-system credential store.
pub struct KeyringYahooCredentialStore;

impl KeyringYahooCredentialStore {
    /// Construct a credential store after confirming a secure platform backend exists.
    pub fn new() -> Result<Self, YahooError> {
        require_secure_backend(keyring::Entry::store_status().is_ok())?;
        Ok(Self)
    }

    /// Return the b9 keyring service name.
    pub fn service_name() -> &'static str {
        KEYRING_SERVICE
    }

    /// Return the b9 Yahoo keyring account name.
    pub fn account_name() -> &'static str {
        KEYRING_ACCOUNT
    }

    fn entry(&self) -> Result<keyring::Entry, YahooError> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
            .map_err(|_| YahooError::Credential("open secure credential entry"))
    }
}

impl YahooCredentialStore for KeyringYahooCredentialStore {
    fn load(&self) -> Result<Option<String>, YahooError> {
        match self.entry()?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(YahooError::Credential("read secure credential entry")),
        }
    }

    fn save(&self, credential: &str) -> Result<(), YahooError> {
        self.entry()?
            .set_password(credential)
            .map_err(|_| YahooError::Credential("write secure credential entry"))
    }

    fn delete(&self) -> Result<(), YahooError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(YahooError::Credential("delete secure credential entry")),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct YahooToken {
    access_token: String,
    refresh_token: String,
    token_type: String,
    expires_at: u64,
}

impl fmt::Debug for YahooToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("YahooToken")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("token_type", &self.token_type)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Default)]
struct TokenState {
    loaded: bool,
    refreshing: bool,
    token: Option<YahooToken>,
}

struct SharedTokenState {
    value: Mutex<TokenState>,
    refreshed: Condvar,
}

/// Yahoo authentication and authenticated raw-request adapter.
pub struct YahooClient {
    http: Arc<HttpClient>,
    endpoints: YahooEndpoints,
    client_id: String,
    credentials: Arc<dyn YahooCredentialStore>,
    clock: Arc<dyn YahooClock>,
    nonces: Arc<dyn YahooNonceSource>,
    waiter: Arc<dyn YahooWaiter>,
    state: SharedTokenState,
}

impl YahooClient {
    /// Construct a Yahoo adapter with injected boundaries.
    pub fn new(
        http: Arc<HttpClient>,
        endpoints: YahooEndpoints,
        client_id: impl Into<String>,
        credentials: Arc<dyn YahooCredentialStore>,
        clock: Arc<dyn YahooClock>,
        nonces: Arc<dyn YahooNonceSource>,
        waiter: Arc<dyn YahooWaiter>,
    ) -> Result<Self, YahooError> {
        let client_id = client_id.into();
        if client_id.trim().is_empty() {
            return Err(YahooError::Configuration(
                "YAHOO_CLIENT_ID must be set and nonblank",
            ));
        }
        Ok(Self {
            http,
            endpoints,
            client_id,
            credentials,
            clock,
            nonces,
            waiter,
            state: SharedTokenState {
                value: Mutex::new(TokenState::default()),
                refreshed: Condvar::new(),
            },
        })
    }

    /// Construct a Yahoo adapter with production boundaries and `YAHOO_CLIENT_ID`.
    pub fn production(http: Arc<HttpClient>) -> Result<Self, YahooError> {
        let endpoints = YahooEndpoints::production();
        let client_id = env::var("YAHOO_CLIENT_ID").unwrap_or_default();
        if client_id.trim().is_empty() {
            return Err(YahooError::Configuration(
                "YAHOO_CLIENT_ID must be set and nonblank",
            ));
        }
        let credentials = Arc::new(KeyringYahooCredentialStore::new()?);
        Self::new(
            http,
            endpoints,
            client_id,
            credentials,
            Arc::new(SystemYahooClock),
            Arc::new(SystemYahooNonceSource),
            Arc::new(ThreadYahooWaiter),
        )
    }

    /// Start one PKCE authorization attempt.
    pub fn begin_authorization(&self) -> Result<AuthorizationStart, YahooError> {
        let state = URL_SAFE_NO_PAD.encode(self.nonces.bytes(32)?);
        let verifier = URL_SAFE_NO_PAD.encode(self.nonces.bytes(32)?);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let mut url = self.endpoints.auth.clone();
        url.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", self.endpoints.redirect.as_str())
            .append_pair("response_type", "code")
            .append_pair("scope", "fspt-r")
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("access_type", "offline");
        Ok(AuthorizationStart {
            url: url.into(),
            pending: PendingAuthorization { state, verifier },
        })
    }

    /// Complete one authorization attempt and persist its initial token.
    pub fn complete_authorization(
        &self,
        pending: PendingAuthorization,
        callback: &str,
    ) -> Result<(), YahooError> {
        let code = self.validate_callback(callback, &pending.state)?;
        let token = self.exchange_token(
            &[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", self.endpoints.redirect.as_str()),
                ("client_id", &self.client_id),
                ("code_verifier", &pending.verifier),
            ],
            None,
        )?;
        self.save_token(&token)?;
        let mut state = self.lock_state()?;
        state.loaded = true;
        state.token = Some(token);
        Ok(())
    }

    /// Return public status for the stored token without refreshing it.
    pub fn token_status(&self) -> Result<YahooTokenStatus, YahooError> {
        let now = self.clock.now();
        let mut state = self.lock_state()?;
        self.load_locked(&mut state)?;
        let Some(token) = &state.token else {
            return Ok(YahooTokenStatus {
                valid: false,
                has_refresh: false,
                expires_at: None,
            });
        };
        Ok(YahooTokenStatus {
            valid: token_valid(token, now),
            has_refresh: !token.refresh_token.trim().is_empty(),
            expires_at: Some(UNIX_EPOCH + Duration::from_secs(token.expires_at)),
        })
    }

    /// Delete the b9 Yahoo credential idempotently.
    pub fn delete_credential(&self) -> Result<(), YahooError> {
        self.credentials.delete()?;
        let mut state = self.lock_state()?;
        state.loaded = true;
        state.token = None;
        Ok(())
    }

    /// Fetch one authenticated Yahoo fantasy path as bounded raw bytes.
    pub fn get_raw(&self, path: &str) -> Result<YahooRawResponse, YahooError> {
        let url = raw_url(&self.endpoints.api, path)?;
        let (token, issue) = self.usable_token()?;
        let mut fallback = Duration::from_secs(1);
        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .http
                .execute(HttpRequest {
                    method: HttpMethod::Get,
                    url: url.to_string(),
                    headers: vec![HttpHeader {
                        name: "Authorization".into(),
                        value: format!("Bearer {}", token.access_token),
                    }],
                    body: Vec::new(),
                    timeout: REQUEST_TIMEOUT,
                    body_limit: RAW_BODY_LIMIT,
                })
                .map_err(|_| YahooError::Request("transport failed"))?;
            if response.status == 429 {
                if attempt + 1 == MAX_ATTEMPTS {
                    return Err(YahooError::RateLimited);
                }
                let wait = retry_after(&response).unwrap_or(fallback);
                self.waiter.wait(wait.min(Duration::from_secs(30)));
                fallback = (fallback * 2).min(Duration::from_secs(30));
                continue;
            }
            if matches!(response.status, 401 | 403) {
                return Err(YahooError::TerminalAccess {
                    status: response.status,
                });
            }
            if response.status != 200 {
                return Err(YahooError::Request(
                    "provider returned an unsuccessful status",
                ));
            }
            return Ok(YahooRawResponse {
                body: response.body,
                issues: issue.into_iter().collect(),
            });
        }
        Err(YahooError::RateLimited)
    }

    fn validate_callback(
        &self,
        callback: &str,
        expected_state: &str,
    ) -> Result<String, YahooError> {
        let url = Url::parse(callback)
            .map_err(|_| YahooError::Authorization("paste the complete callback URL"))?;
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err(YahooError::Authorization("callback URL is malformed"));
        }
        if origin_and_path(&url) != origin_and_path(&self.endpoints.redirect) {
            return Err(YahooError::Authorization("callback target does not match"));
        }
        let pairs: Vec<_> = url.query_pairs().collect();
        if pairs.iter().any(|(key, _)| key == "error") {
            return Err(YahooError::Authorization("provider rejected authorization"));
        }
        let states: Vec<_> = pairs.iter().filter(|(key, _)| key == "state").collect();
        if states.len() != 1 || states[0].1.as_ref() != expected_state {
            return Err(YahooError::Authorization("callback state does not match"));
        }
        let codes: Vec<_> = pairs.iter().filter(|(key, _)| key == "code").collect();
        if codes.len() != 1 || codes[0].1.trim().is_empty() {
            return Err(YahooError::Authorization(
                "callback must contain exactly one authorization code",
            ));
        }
        Ok(codes[0].1.to_string())
    }

    fn usable_token(&self) -> Result<(YahooToken, Option<YahooIssue>), YahooError> {
        loop {
            let now = self.clock.now();
            let mut state = self.lock_state()?;
            self.load_locked(&mut state)?;
            let Some(token) = state.token.clone() else {
                return Err(YahooError::NotAuthenticated);
            };
            if token_valid(&token, now) {
                return Ok((token, None));
            }
            if token.refresh_token.trim().is_empty() {
                return Err(YahooError::SessionExpired);
            }
            if state.refreshing {
                state = self
                    .state
                    .refreshed
                    .wait(state)
                    .map_err(|_| YahooError::Credential("token state is unavailable"))?;
                drop(state);
                continue;
            }
            state.refreshing = true;
            drop(state);

            let result = self.refresh_token(&token);
            let mut state = self.lock_state()?;
            state.refreshing = false;
            match result {
                Ok((refreshed, issue)) => {
                    state.token = Some(refreshed.clone());
                    self.state.refreshed.notify_all();
                    return Ok((refreshed, issue));
                }
                Err(error) => {
                    self.state.refreshed.notify_all();
                    return Err(error);
                }
            }
        }
    }

    fn refresh_token(
        &self,
        previous: &YahooToken,
    ) -> Result<(YahooToken, Option<YahooIssue>), YahooError> {
        let mut token = self.exchange_token(
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &previous.refresh_token),
                ("redirect_uri", self.endpoints.redirect.as_str()),
                ("client_id", &self.client_id),
            ],
            Some(&previous.refresh_token),
        )?;
        if token.refresh_token.is_empty() {
            token.refresh_token.clone_from(&previous.refresh_token);
        }
        let issue = self
            .save_token(&token)
            .err()
            .map(|_| YahooIssue::CredentialPersistence);
        Ok((token, issue))
    }

    fn exchange_token(
        &self,
        fields: &[(&str, &str)],
        prior_refresh: Option<&str>,
    ) -> Result<YahooToken, YahooError> {
        let body = form_body(fields);
        let response = self
            .http
            .execute(HttpRequest {
                method: HttpMethod::Post,
                url: self.endpoints.token.to_string(),
                headers: vec![HttpHeader {
                    name: "Content-Type".into(),
                    value: "application/x-www-form-urlencoded".into(),
                }],
                body,
                timeout: REQUEST_TIMEOUT,
                body_limit: TOKEN_BODY_LIMIT,
            })
            .map_err(|_| YahooError::TokenResponse("token request failed"))?;
        if response.status != 200 {
            return Err(YahooError::TokenResponse(
                "provider returned an unsuccessful status",
            ));
        }
        let decoded: TokenEnvelope = serde_json::from_slice(&response.body)
            .map_err(|_| YahooError::TokenResponse("token response is malformed"))?;
        if decoded.access_token.trim().is_empty()
            || !decoded.token_type.eq_ignore_ascii_case("bearer")
            || decoded.expires_in == 0
        {
            return Err(YahooError::TokenResponse(
                "token response contains invalid fields",
            ));
        }
        let now = self.clock.now();
        let now_seconds = now
            .duration_since(UNIX_EPOCH)
            .map_err(|_| YahooError::TokenResponse("clock precedes the Unix epoch"))?
            .as_secs();
        let expires_at = now_seconds
            .checked_add(decoded.expires_in)
            .ok_or(YahooError::TokenResponse("token expiry overflows"))?;
        UNIX_EPOCH
            .checked_add(Duration::from_secs(expires_at))
            .ok_or(YahooError::TokenResponse("token expiry overflows"))?;
        Ok(YahooToken {
            access_token: decoded.access_token,
            refresh_token: decoded
                .refresh_token
                .or_else(|| prior_refresh.map(str::to_owned))
                .unwrap_or_default(),
            token_type: "Bearer".into(),
            expires_at,
        })
    }

    fn save_token(&self, token: &YahooToken) -> Result<(), YahooError> {
        let encoded = serde_json::to_string(token)
            .map_err(|_| YahooError::Credential("serialize Yahoo credential"))?;
        self.credentials.save(&encoded)
    }

    fn load_locked(&self, state: &mut TokenState) -> Result<(), YahooError> {
        if state.loaded {
            return Ok(());
        }
        state.token = match self.credentials.load()? {
            None => None,
            Some(encoded) => {
                let token: YahooToken = serde_json::from_str(&encoded)
                    .map_err(|_| YahooError::Credential("stored credential is malformed"))?;
                validate_stored_token(&token)?;
                Some(token)
            }
        };
        state.loaded = true;
        Ok(())
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, TokenState>, YahooError> {
        self.state
            .value
            .lock()
            .map_err(|_| YahooError::Credential("token state is unavailable"))
    }
}

#[derive(Deserialize)]
struct TokenEnvelope {
    access_token: String,
    token_type: String,
    expires_in: u64,
    #[serde(default)]
    refresh_token: Option<String>,
}

fn validate_endpoint(detail: &'static str, value: &str) -> Result<Url, YahooError> {
    let url = Url::parse(value).map_err(|_| YahooError::Configuration(detail))?;
    let loopback_http = url.scheme() == "http" && url.host_str().is_some_and(is_loopback);
    if url.scheme() != "https" && !loopback_http {
        return Err(YahooError::Configuration(
            "production endpoints must use HTTPS",
        ));
    }
    if url.username() != "" || url.password().is_some() || url.fragment().is_some() {
        return Err(YahooError::Configuration(detail));
    }
    Ok(url)
}

fn validate_redirect(value: &str) -> Result<Url, YahooError> {
    let url = validate_endpoint("redirect URI is invalid", value)?;
    if url.query().is_some() || url.path() == "/" {
        return Err(YahooError::Configuration("redirect URI is invalid"));
    }
    Ok(url)
}

fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn origin_and_path(url: &Url) -> (String, String, Option<u16>, String) {
    (
        url.scheme().into(),
        url.host_str().unwrap_or_default().into(),
        url.port_or_known_default(),
        url.path().into(),
    )
}

fn form_body(fields: &[(&str, &str)]) -> Vec<u8> {
    let mut url = Url::parse("https://form.invalid/").expect("static form URL is valid");
    for (key, value) in fields {
        url.query_pairs_mut().append_pair(key, value);
    }
    url.query().unwrap_or_default().as_bytes().to_vec()
}

fn token_valid(token: &YahooToken, now: SystemTime) -> bool {
    let Ok(now) = now.duration_since(UNIX_EPOCH) else {
        return false;
    };
    now.as_secs()
        .checked_add(EXPIRY_MARGIN.as_secs())
        .is_some_and(|threshold| threshold < token.expires_at)
}

fn validate_stored_token(token: &YahooToken) -> Result<(), YahooError> {
    if token.access_token.trim().is_empty()
        || !token.token_type.eq_ignore_ascii_case("bearer")
        || token.expires_at == 0
        || UNIX_EPOCH
            .checked_add(Duration::from_secs(token.expires_at))
            .is_none()
    {
        return Err(YahooError::Credential("stored credential is malformed"));
    }
    Ok(())
}

fn raw_url(root: &Url, path: &str) -> Result<Url, YahooError> {
    if !path.starts_with('/')
        || path.starts_with("//")
        || path.contains('#')
        || path.contains('\\')
        || path.trim().is_empty()
    {
        return Err(YahooError::InvalidPath("provider path is unsafe"));
    }
    let (raw_path, query) = path.split_once('?').unwrap_or((path, ""));
    if !valid_percent_encoding(raw_path) || raw_path.split('/').any(unsafe_segment) {
        return Err(YahooError::InvalidPath(
            "provider path is malformed or contains traversal",
        ));
    }
    let mut url = root.clone();
    let base = root.path().trim_end_matches('/');
    url.set_path(&format!("{base}{raw_path}"));
    url.set_query(if query.is_empty() { None } else { Some(query) });
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| key != "format")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    url.set_query(None);
    {
        let mut output = url.query_pairs_mut();
        for (key, value) in pairs {
            output.append_pair(&key, &value);
        }
        output.append_pair("format", "json");
    }
    Ok(url)
}

fn valid_percent_encoding(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || hex(bytes[index + 1]).is_none()
                || hex(bytes[index + 2]).is_none()
            {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}

fn unsafe_segment(segment: &str) -> bool {
    let decoded = percent_decode(segment);
    decoded == "." || decoded == ".." || decoded.contains('/') || decoded.contains('\\')
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            output.push((high << 4) | low);
            index += 3;
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn retry_after(response: &HttpResponse) -> Option<Duration> {
    response
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("retry-after"))
        .and_then(|header| header.value.parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn require_secure_backend(available: bool) -> Result<(), YahooError> {
    if available {
        Ok(())
    } else {
        Err(YahooError::Credential(
            "secure credential backend is unavailable",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{YahooError, require_secure_backend};

    #[test]
    fn unsupported_secure_backend_fails_without_fallback() {
        assert_eq!(require_secure_backend(true), Ok(()));
        assert_eq!(
            require_secure_backend(false),
            Err(YahooError::Credential(
                "secure credential backend is unavailable"
            ))
        );
    }
}
