use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use b9::providers::yahoo::{
    KeyringYahooCredentialStore, YahooClient, YahooClock, YahooCredentialStore, YahooEndpoints,
    YahooError, YahooIssue, YahooNonceSource, YahooWaiter,
};
use b9::transport::{
    ExecutorError, HttpClient, HttpExecutor, HttpHeader, HttpResponse, ValidatedRequest,
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Url;
use sha2::{Digest, Sha256};

const TOKEN_SUCCESS: &[u8] = include_bytes!("fixtures/yahoo/token-success.json");

// Fixture provenance: scrubbed from Skout's documented Yahoo OAuth token shape.

#[derive(Clone, Debug)]
struct CapturedRequest {
    method: String,
    url: String,
    headers: Vec<HttpHeader>,
    body: Vec<u8>,
    timeout: Duration,
    body_limit: usize,
}

struct FakeExecutor {
    responses: Mutex<VecDeque<Result<HttpResponse, ExecutorError>>>,
    requests: Mutex<Vec<CapturedRequest>>,
}

impl FakeExecutor {
    fn new(responses: Vec<Result<HttpResponse, ExecutorError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<CapturedRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl HttpExecutor for FakeExecutor {
    fn execute(&self, request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
        self.requests.lock().unwrap().push(CapturedRequest {
            method: format!("{:?}", request.method()),
            url: request.url().into(),
            headers: request.headers(),
            body: request.body().to_vec(),
            timeout: request.timeout(),
            body_limit: request.body_limit(),
        });
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("fake response available")
    }
}

#[derive(Default)]
struct FakeCredentials {
    value: Mutex<Option<String>>,
    load_error: Mutex<bool>,
    save_error: Mutex<bool>,
    delete_error: Mutex<bool>,
    saves: AtomicUsize,
}

impl FakeCredentials {
    fn with_value(value: impl Into<String>) -> Self {
        Self {
            value: Mutex::new(Some(value.into())),
            ..Self::default()
        }
    }
}

impl YahooCredentialStore for FakeCredentials {
    fn load(&self) -> Result<Option<String>, YahooError> {
        if *self.load_error.lock().unwrap() {
            return Err(YahooError::Credential("synthetic load failure"));
        }
        Ok(self.value.lock().unwrap().clone())
    }

    fn save(&self, credential: &str) -> Result<(), YahooError> {
        self.saves.fetch_add(1, Ordering::SeqCst);
        if *self.save_error.lock().unwrap() {
            return Err(YahooError::Credential("synthetic save failure"));
        }
        *self.value.lock().unwrap() = Some(credential.into());
        Ok(())
    }

    fn delete(&self) -> Result<(), YahooError> {
        if *self.delete_error.lock().unwrap() {
            return Err(YahooError::Credential("synthetic delete failure"));
        }
        *self.value.lock().unwrap() = None;
        Ok(())
    }
}

struct FakeClock {
    now: Mutex<SystemTime>,
    calls: AtomicUsize,
}

impl FakeClock {
    fn new(seconds: u64) -> Self {
        Self {
            now: Mutex::new(UNIX_EPOCH + Duration::from_secs(seconds)),
            calls: AtomicUsize::new(0),
        }
    }
}

impl YahooClock for FakeClock {
    fn now(&self) -> SystemTime {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.now.lock().unwrap()
    }
}

struct FakeNonces {
    values: Mutex<VecDeque<Vec<u8>>>,
}

impl FakeNonces {
    fn new(values: Vec<Vec<u8>>) -> Self {
        Self {
            values: Mutex::new(values.into()),
        }
    }
}

impl YahooNonceSource for FakeNonces {
    fn bytes(&self, length: usize) -> Result<Vec<u8>, YahooError> {
        let value = self.values.lock().unwrap().pop_front().unwrap();
        assert_eq!(value.len(), length);
        Ok(value)
    }
}

#[derive(Default)]
struct FakeWaiter {
    waits: Mutex<Vec<Duration>>,
}

impl YahooWaiter for FakeWaiter {
    fn wait(&self, duration: Duration) {
        self.waits.lock().unwrap().push(duration);
    }
}

fn endpoints() -> YahooEndpoints {
    YahooEndpoints::new(
        "http://127.0.0.1/oauth/authorize",
        "http://127.0.0.1/oauth/token",
        "http://127.0.0.1/fantasy/v2",
        "http://127.0.0.1/callback",
    )
    .unwrap()
}

fn response(status: u16, body: impl Into<Vec<u8>>) -> Result<HttpResponse, ExecutorError> {
    Ok(HttpResponse {
        status,
        headers: Vec::new(),
        body: body.into(),
    })
}

fn client(
    executor: Arc<FakeExecutor>,
    credentials: Arc<FakeCredentials>,
    clock: Arc<FakeClock>,
    nonces: Arc<FakeNonces>,
    waiter: Arc<FakeWaiter>,
) -> YahooClient {
    YahooClient::new(
        Arc::new(HttpClient::new(executor)),
        endpoints(),
        "test-client-id",
        credentials,
        clock,
        nonces,
        waiter,
    )
    .unwrap()
}

fn token(access: &str, refresh: &str, expires_at: u64) -> String {
    serde_json::json!({
        "access_token": access,
        "refresh_token": refresh,
        "token_type": "Bearer",
        "expires_at": expires_at
    })
    .to_string()
}

fn auth_header(request: &CapturedRequest) -> Option<&str> {
    request
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("authorization"))
        .map(|header| header.value.as_str())
}

#[test]
fn configuration_rejects_invalid_values_before_side_effects() {
    assert!(
        YahooEndpoints::new(
            "http://example.com/a",
            "https://x/t",
            "https://x/a",
            "https://localhost/c"
        )
        .is_err()
    );
    assert!(
        YahooEndpoints::new(
            "https://x/a",
            "https://x/t",
            "https://x/a",
            "https://localhost/"
        )
        .is_err()
    );
    let executor = Arc::new(FakeExecutor::new(Vec::new()));
    let credentials = Arc::new(FakeCredentials::default());
    let result = YahooClient::new(
        Arc::new(HttpClient::new(executor.clone())),
        endpoints(),
        "  ",
        credentials.clone(),
        Arc::new(FakeClock::new(1)),
        Arc::new(FakeNonces::new(Vec::new())),
        Arc::new(FakeWaiter::default()),
    );
    assert!(matches!(result, Err(YahooError::Configuration(_))));
    assert!(executor.requests().is_empty());
    assert_eq!(credentials.saves.load(Ordering::SeqCst), 0);
}

#[test]
fn authorization_uses_unique_pkce_state_and_redacts_debug() {
    let executor = Arc::new(FakeExecutor::new(Vec::new()));
    let nonces = Arc::new(FakeNonces::new(vec![
        vec![1; 32],
        vec![2; 32],
        vec![3; 32],
        vec![4; 32],
    ]));
    let client = client(
        executor,
        Arc::new(FakeCredentials::default()),
        Arc::new(FakeClock::new(10)),
        nonces,
        Arc::new(FakeWaiter::default()),
    );
    let first = client.begin_authorization().unwrap();
    let second = client.begin_authorization().unwrap();
    let first_url = Url::parse(&first.url).unwrap();
    let pairs: std::collections::HashMap<_, _> = first_url.query_pairs().into_owned().collect();
    let verifier = URL_SAFE_NO_PAD.encode(vec![2; 32]);
    assert_eq!(pairs["state"], URL_SAFE_NO_PAD.encode(vec![1; 32]));
    assert_eq!(
        pairs["code_challenge"],
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
    );
    assert_eq!(pairs["code_challenge_method"], "S256");
    assert_eq!(pairs["scope"], "fspt-r");
    assert_eq!(pairs["access_type"], "offline");
    assert_ne!(first.url, second.url);
    let debug = format!("{first:?}");
    assert!(!debug.contains("test-client-id"));
    assert!(!debug.contains(&verifier));
}

#[test]
fn complete_authorization_validates_callback_and_persists_token() {
    let executor = Arc::new(FakeExecutor::new(vec![response(200, TOKEN_SUCCESS)]));
    let credentials = Arc::new(FakeCredentials::default());
    let clock = Arc::new(FakeClock::new(1_000));
    let nonces = Arc::new(FakeNonces::new(vec![vec![5; 32], vec![6; 32]]));
    let client = client(
        executor.clone(),
        credentials.clone(),
        clock.clone(),
        nonces,
        Arc::new(FakeWaiter::default()),
    );
    let start = client.begin_authorization().unwrap();
    let state = Url::parse(&start.url)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    client
        .complete_authorization(
            start.pending,
            &format!("http://127.0.0.1/callback?code=code-value&state={state}"),
        )
        .unwrap();
    let requests = executor.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "Post");
    assert_eq!(requests[0].timeout, Duration::from_secs(10));
    assert_eq!(requests[0].body_limit, 64 * 1024);
    let body = String::from_utf8(requests[0].body.clone()).unwrap();
    assert!(body.contains("grant_type=authorization_code"));
    assert!(body.contains("client_id=test-client-id"));
    assert!(body.contains("code=code-value"));
    assert!(body.contains("code_verifier="));
    assert!(auth_header(&requests[0]).is_none());
    let stored: serde_json::Value =
        serde_json::from_str(credentials.value.lock().unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(stored["expires_at"], 4_600);
    assert_eq!(clock.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn callback_rejections_do_not_exchange_tokens() {
    let executor = Arc::new(FakeExecutor::new(Vec::new()));
    let client = client(
        executor.clone(),
        Arc::new(FakeCredentials::default()),
        Arc::new(FakeClock::new(1)),
        Arc::new(FakeNonces::new(vec![
            vec![1; 32],
            vec![2; 32],
            vec![3; 32],
            vec![4; 32],
            vec![5; 32],
            vec![6; 32],
            vec![7; 32],
            vec![8; 32],
            vec![9; 32],
            vec![10; 32],
            vec![11; 32],
            vec![12; 32],
        ])),
        Arc::new(FakeWaiter::default()),
    );
    for callback in [
        "bare-code",
        "http://127.0.0.1/wrong?code=x&state=y",
        "http://127.0.0.1/callback?code=x",
        "http://127.0.0.1/callback?code=x&code=y&state=z",
        "http://127.0.0.1/callback?error=denied&state=z",
        "http://127.0.0.1/callback?code=x&state=z#fragment",
    ] {
        let start = client.begin_authorization().unwrap();
        assert!(
            client
                .complete_authorization(start.pending, callback)
                .is_err()
        );
    }
    assert!(executor.requests().is_empty());
}

#[test]
fn token_response_and_initial_save_failures_are_safe() {
    for body in [
        br#"{}"#.as_slice(),
        br#"{"access_token":"","token_type":"Bearer","expires_in":1}"#.as_slice(),
        br#"{"access_token":"secret","token_type":"MAC","expires_in":1}"#.as_slice(),
        br#"{"access_token":"secret","token_type":"Bearer","expires_in":0}"#.as_slice(),
    ] {
        let executor = Arc::new(FakeExecutor::new(vec![response(200, body)]));
        let client = client(
            executor,
            Arc::new(FakeCredentials::default()),
            Arc::new(FakeClock::new(1)),
            Arc::new(FakeNonces::new(vec![vec![1; 32], vec![2; 32]])),
            Arc::new(FakeWaiter::default()),
        );
        let start = client.begin_authorization().unwrap();
        let state = Url::parse(&start.url)
            .unwrap()
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        let error = client
            .complete_authorization(
                start.pending,
                &format!("http://127.0.0.1/callback?code=secret-code&state={state}"),
            )
            .unwrap_err();
        let text = format!("{error:?} {error}");
        assert!(!text.contains("secret-code"));
        assert!(!text.contains("secret\""));
    }
    let executor = Arc::new(FakeExecutor::new(vec![response(200, TOKEN_SUCCESS)]));
    let credentials = Arc::new(FakeCredentials::default());
    *credentials.save_error.lock().unwrap() = true;
    let save_failure_client = client(
        executor,
        credentials,
        Arc::new(FakeClock::new(1)),
        Arc::new(FakeNonces::new(vec![vec![1; 32], vec![2; 32]])),
        Arc::new(FakeWaiter::default()),
    );
    let start = save_failure_client.begin_authorization().unwrap();
    let state = Url::parse(&start.url)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    assert!(matches!(
        save_failure_client.complete_authorization(
            start.pending,
            &format!("http://127.0.0.1/callback?code=x&state={state}")
        ),
        Err(YahooError::Credential(_))
    ));

    for token_result in [
        response(400, b"provider-secret"),
        response(
            200,
            br#"{"access_token":"secret","token_type":"Bearer","expires_in":18446744073709551615}"#,
        ),
        Err(ExecutorError::ResponseTooLarge { limit: 64 * 1024 }),
    ] {
        let executor = Arc::new(FakeExecutor::new(vec![token_result]));
        let client = client(
            executor,
            Arc::new(FakeCredentials::default()),
            Arc::new(FakeClock::new(1)),
            Arc::new(FakeNonces::new(vec![vec![1; 32], vec![2; 32]])),
            Arc::new(FakeWaiter::default()),
        );
        let start = client.begin_authorization().unwrap();
        let state = Url::parse(&start.url)
            .unwrap()
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        let error = client
            .complete_authorization(
                start.pending,
                &format!("http://127.0.0.1/callback?code=secret-code&state={state}"),
            )
            .unwrap_err();
        let text = format!("{error:?} {error}");
        assert!(!text.contains("provider-secret"));
        assert!(!text.contains("secret-code"));
    }
}

#[test]
fn credential_status_distinguishes_absent_malformed_and_failure() {
    assert_eq!(KeyringYahooCredentialStore::service_name(), "b9");
    assert_eq!(
        KeyringYahooCredentialStore::account_name(),
        "yahoo-oauth-token"
    );
    let executor = Arc::new(FakeExecutor::new(Vec::new()));
    let credentials = Arc::new(FakeCredentials::default());
    let absent_client = client(
        executor.clone(),
        credentials.clone(),
        Arc::new(FakeClock::new(100)),
        Arc::new(FakeNonces::new(Vec::new())),
        Arc::new(FakeWaiter::default()),
    );
    assert!(!absent_client.token_status().unwrap().valid);
    absent_client.delete_credential().unwrap();
    absent_client.delete_credential().unwrap();

    for encoded in ["not-json".into(), token("", "refresh", 1_000)] {
        let malformed = client(
            executor.clone(),
            Arc::new(FakeCredentials::with_value(encoded)),
            Arc::new(FakeClock::new(100)),
            Arc::new(FakeNonces::new(Vec::new())),
            Arc::new(FakeWaiter::default()),
        );
        assert!(matches!(
            malformed.token_status(),
            Err(YahooError::Credential(_))
        ));
    }
    let failed = Arc::new(FakeCredentials::default());
    *failed.load_error.lock().unwrap() = true;
    let client = client(
        executor,
        failed,
        Arc::new(FakeClock::new(100)),
        Arc::new(FakeNonces::new(Vec::new())),
        Arc::new(FakeWaiter::default()),
    );
    assert!(matches!(
        client.token_status(),
        Err(YahooError::Credential(_))
    ));
}

#[test]
fn refresh_preserves_rotating_token_and_surfaces_save_issue() {
    let refreshed = br#"{"access_token":"new-access","token_type":"bearer","expires_in":100}"#;
    let executor = Arc::new(FakeExecutor::new(vec![
        response(200, refreshed),
        response(200, b"raw"),
    ]));
    let credentials = Arc::new(FakeCredentials::with_value(token(
        "old-access",
        "keep-refresh",
        109,
    )));
    *credentials.save_error.lock().unwrap() = true;
    let client = client(
        executor.clone(),
        credentials,
        Arc::new(FakeClock::new(100)),
        Arc::new(FakeNonces::new(Vec::new())),
        Arc::new(FakeWaiter::default()),
    );
    let result = client.get_raw("/league/one?week=2&format=xml").unwrap();
    assert_eq!(result.body, b"raw");
    assert_eq!(result.issues, vec![YahooIssue::CredentialPersistence]);
    let requests = executor.requests();
    let refresh_body = String::from_utf8_lossy(&requests[0].body);
    assert!(refresh_body.contains("grant_type=refresh_token"));
    assert!(refresh_body.contains("refresh_token=keep-refresh"));
    assert!(refresh_body.contains("client_id=test-client-id"));
    assert!(refresh_body.contains("redirect_uri="));
    assert_eq!(requests[0].timeout, Duration::from_secs(10));
    assert_eq!(requests[0].body_limit, 64 * 1024);
    assert!(auth_header(&requests[0]).is_none());
    assert_eq!(auth_header(&requests[1]), Some("Bearer new-access"));
    assert!(requests[1].url.contains("week=2"));
    assert_eq!(requests[1].url.matches("format=json").count(), 1);
}

#[test]
fn valid_token_bypasses_refresh_and_unsafe_paths_fail_closed() {
    let executor = Arc::new(FakeExecutor::new(vec![response(200, b"ok")]));
    let client = client(
        executor.clone(),
        Arc::new(FakeCredentials::with_value(token(
            "access", "refresh", 1_000,
        ))),
        Arc::new(FakeClock::new(100)),
        Arc::new(FakeNonces::new(Vec::new())),
        Arc::new(FakeWaiter::default()),
    );
    for path in [
        "https://evil.example/x",
        "//evil.example/x",
        "/../x",
        "/%2e%2e/x",
        "/%GG/x",
        "/%2/x",
        "/x#fragment",
        "/x\\y",
    ] {
        assert!(client.get_raw(path).is_err());
    }
    let result = client.get_raw("/league/one?week=2").unwrap();
    assert_eq!(result.body, b"ok");
    let requests = executor.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].timeout, Duration::from_secs(10));
    assert_eq!(requests[0].body_limit, 8 * 1024 * 1024);
}

#[test]
fn concurrent_requests_singleflight_refresh() {
    let responses = vec![
        response(200, TOKEN_SUCCESS),
        response(200, b"one"),
        response(200, b"two"),
        response(200, b"three"),
    ];
    let executor = Arc::new(FakeExecutor::new(responses));
    let credentials = Arc::new(FakeCredentials::with_value(token("old", "refresh", 1)));
    let client = Arc::new(client(
        executor.clone(),
        credentials.clone(),
        Arc::new(FakeClock::new(100)),
        Arc::new(FakeNonces::new(Vec::new())),
        Arc::new(FakeWaiter::default()),
    ));
    let barrier = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..3)
        .map(|_| {
            let client = client.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                client.get_raw("/league/one").unwrap()
            })
        })
        .collect();
    barrier.wait();
    for handle in handles {
        assert!(!handle.join().unwrap().body.is_empty());
    }
    let requests = executor.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.method == "Post")
            .count(),
        1
    );
    assert_eq!(credentials.saves.load(Ordering::SeqCst), 1);
    assert!(
        requests
            .iter()
            .filter(|request| request.method == "Get")
            .all(|request| auth_header(request) == Some("Bearer fixture-access-token"))
    );
}

#[test]
fn rate_limits_retry_with_bounded_waits() {
    let mut limited = Vec::new();
    for retry in ["99", "bad", "bad", "bad"] {
        limited.push(Ok(HttpResponse {
            status: 429,
            headers: vec![HttpHeader {
                name: "Retry-After".into(),
                value: retry.into(),
            }],
            body: b"secret-body".to_vec(),
        }));
    }
    limited.push(response(200, b"ok"));
    let executor = Arc::new(FakeExecutor::new(limited));
    let waiter = Arc::new(FakeWaiter::default());
    let client = client(
        executor,
        Arc::new(FakeCredentials::with_value(token(
            "access", "refresh", 1_000,
        ))),
        Arc::new(FakeClock::new(100)),
        Arc::new(FakeNonces::new(Vec::new())),
        waiter.clone(),
    );
    assert_eq!(client.get_raw("/x").unwrap().body, b"ok");
    assert_eq!(
        *waiter.waits.lock().unwrap(),
        vec![
            Duration::from_secs(30),
            Duration::from_secs(2),
            Duration::from_secs(4),
            Duration::from_secs(8)
        ]
    );
}

#[test]
fn exhausted_rate_limit_and_transport_failures_are_bounded() {
    let executor = Arc::new(FakeExecutor::new(
        (0..5).map(|_| response(429, b"private")).collect(),
    ));
    let waiter = Arc::new(FakeWaiter::default());
    let rate_limited_client = client(
        executor.clone(),
        Arc::new(FakeCredentials::with_value(token(
            "access", "refresh", 1_000,
        ))),
        Arc::new(FakeClock::new(100)),
        Arc::new(FakeNonces::new(Vec::new())),
        waiter.clone(),
    );
    assert_eq!(
        rate_limited_client.get_raw("/x").unwrap_err(),
        YahooError::RateLimited
    );
    assert_eq!(executor.requests().len(), 5);
    assert_eq!(waiter.waits.lock().unwrap().len(), 4);

    let executor = Arc::new(FakeExecutor::new(vec![Err(ExecutorError::Timeout {
        url: "https://secret.invalid/token".into(),
    })]));
    let client = client(
        executor,
        Arc::new(FakeCredentials::with_value(token(
            "access", "refresh", 1_000,
        ))),
        Arc::new(FakeClock::new(100)),
        Arc::new(FakeNonces::new(Vec::new())),
        Arc::new(FakeWaiter::default()),
    );
    assert_eq!(
        client.get_raw("/x").unwrap_err(),
        YahooError::Request("transport failed")
    );
}

#[test]
fn terminal_and_other_statuses_are_typed_and_secret_safe() {
    for status in [401, 403] {
        let executor = Arc::new(FakeExecutor::new(vec![response(
            status,
            b"secret response body",
        )]));
        let client = client(
            executor.clone(),
            Arc::new(FakeCredentials::with_value(token(
                "secret-access",
                "secret-refresh",
                1_000,
            ))),
            Arc::new(FakeClock::new(100)),
            Arc::new(FakeNonces::new(Vec::new())),
            Arc::new(FakeWaiter::default()),
        );
        let error = client.get_raw("/x").unwrap_err();
        assert!(error.is_terminal_access());
        let text = format!("{error:?} {error}");
        assert!(!text.contains("secret response body"));
        assert!(!text.contains("secret-access"));
        assert_eq!(executor.requests().len(), 1);
    }
    let executor = Arc::new(FakeExecutor::new(vec![response(
        500,
        b"private provider detail",
    )]));
    let client = client(
        executor,
        Arc::new(FakeCredentials::with_value(token(
            "access", "refresh", 1_000,
        ))),
        Arc::new(FakeClock::new(100)),
        Arc::new(FakeNonces::new(Vec::new())),
        Arc::new(FakeWaiter::default()),
    );
    assert!(
        !client
            .get_raw("/x")
            .unwrap_err()
            .to_string()
            .contains("private provider detail")
    );
}

#[test]
fn missing_and_unrefreshable_tokens_name_b9_login() {
    let missing = client(
        Arc::new(FakeExecutor::new(Vec::new())),
        Arc::new(FakeCredentials::default()),
        Arc::new(FakeClock::new(100)),
        Arc::new(FakeNonces::new(Vec::new())),
        Arc::new(FakeWaiter::default()),
    );
    assert_eq!(
        missing.get_raw("/x").unwrap_err().to_string(),
        "not authenticated — run: b9 login"
    );
    let expired = client(
        Arc::new(FakeExecutor::new(Vec::new())),
        Arc::new(FakeCredentials::with_value(token("access", "", 1))),
        Arc::new(FakeClock::new(100)),
        Arc::new(FakeNonces::new(Vec::new())),
        Arc::new(FakeWaiter::default()),
    );
    assert_eq!(
        expired.get_raw("/x").unwrap_err().to_string(),
        "session expired — run: b9 login"
    );
}
