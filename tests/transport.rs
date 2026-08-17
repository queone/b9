use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use b9::transport::{
    ExecutorError, HttpClient, HttpExecutor, HttpHeader, HttpMethod, HttpRequest, HttpResponse,
    ValidatedRequest,
};

#[derive(Default)]
struct RecordingExecutor {
    calls: AtomicUsize,
    request: Mutex<Option<ValidatedRequest>>,
}

impl HttpExecutor for RecordingExecutor {
    fn execute(&self, request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.request.lock().unwrap() = Some(request);
        Ok(HttpResponse {
            status: 418,
            headers: vec![
                HttpHeader {
                    name: "set-cookie".into(),
                    value: "a=1".into(),
                },
                HttpHeader {
                    name: "set-cookie".into(),
                    value: "b=2".into(),
                },
            ],
            body: vec![0, 255],
        })
    }
}

fn request(url: String) -> HttpRequest {
    HttpRequest {
        method: HttpMethod::Get,
        url,
        headers: vec![
            HttpHeader {
                name: "X-Test".into(),
                value: "one".into(),
            },
            HttpHeader {
                name: "X-Test".into(),
                value: "two".into(),
            },
        ],
        body: Vec::new(),
        timeout: Duration::from_secs(2),
        body_limit: 1024,
    }
}

#[test]
fn validating_client_preserves_exact_injected_contract() {
    let executor = Arc::new(RecordingExecutor::default());
    let client = HttpClient::new(executor.clone());
    let response = client
        .execute(request("https://example.test/path".into()))
        .unwrap();
    assert_eq!(response.status, 418);
    assert_eq!(response.body, vec![0, 255]);
    assert_eq!(response.headers.len(), 2);
    let recorded = executor.request.lock().unwrap();
    let recorded = recorded.as_ref().unwrap();
    assert_eq!(recorded.url(), "https://example.test/path");
    assert_eq!(recorded.headers().len(), 2);
    assert_eq!(recorded.timeout(), Duration::from_secs(2));
    assert_eq!(recorded.body_limit(), 1024);
}

#[test]
fn invalid_requests_never_dispatch_and_debug_redacts() {
    let executor = Arc::new(RecordingExecutor::default());
    let client = HttpClient::new(executor.clone());
    for mut invalid_request in [
        request("http://example.test".into()),
        request("https://user:pass@example.test".into()),
        request("https://example.test/#fragment".into()),
    ] {
        assert!(client.execute(invalid_request.clone()).is_err());
        invalid_request.timeout = Duration::ZERO;
        assert!(client.execute(invalid_request).is_err());
    }
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    assert!(client.execute(request("http://192.0.2.1".into())).is_err());
    let mut zero_limit = request("https://example.test".into());
    zero_limit.body_limit = 0;
    assert!(client.execute(zero_limit).is_err());
    let mut bad_header = request("https://example.test".into());
    bad_header.headers.push(HttpHeader {
        name: "bad header".into(),
        value: "value".into(),
    });
    assert!(client.execute(bad_header).is_err());
    let mut get_body = request("https://example.test".into());
    get_body.body = b"not-allowed".to_vec();
    assert!(client.execute(get_body).is_err());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    let secret = HttpRequest {
        headers: vec![
            HttpHeader {
                name: "Authorization".into(),
                value: "Bearer private".into(),
            },
            HttpHeader {
                name: "x-api-key".into(),
                value: "private-key".into(),
            },
        ],
        body: b"secret-body".to_vec(),
        ..request("https://example.test".into())
    };
    let debug = format!("{secret:?}");
    let display = secret.to_string();
    assert!(!debug.contains("private") && !debug.contains("secret-body"));
    assert!(!display.contains("private") && !display.contains("secret-body"));
}

fn serve_once(response: &'static [u8]) -> (String, thread::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = vec![0; 8192];
        let length = stream.read(&mut request).unwrap();
        stream.write_all(response).unwrap();
        request.truncate(length);
        request
    });
    (format!("http://{address}/start"), handle)
}

#[test]
fn production_executor_preserves_status_duplicates_and_body() {
    let (url, server) = serve_once(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 3\r\nX-A: one\r\nX-A: two\r\nConnection: close\r\n\r\nbad");
    let client = HttpClient::production().unwrap();
    let response = client.execute(request(url)).unwrap();
    assert_eq!(response.status, 503);
    assert_eq!(response.body, b"bad");
    assert_eq!(
        response
            .headers
            .iter()
            .filter(|header| header.name == "x-a")
            .count(),
        2
    );
    server.join().unwrap();
}

#[test]
fn production_executor_rejects_declared_and_streamed_oversize_bodies() {
    let client = HttpClient::production().unwrap();
    let (url, server) =
        serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n12345");
    let mut limited = request(url);
    limited.body_limit = 4;
    assert!(client.execute(limited).is_err());
    server.join().unwrap();

    let (url, server) = serve_once(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n12345");
    let mut limited = request(url);
    limited.body_limit = 4;
    assert!(client.execute(limited).is_err());
    server.join().unwrap();
}

#[test]
fn post_redirect_is_returned_without_following() {
    let (url, server) = serve_once(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/unused\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let client = HttpClient::production().unwrap();
    let mut post = request(url);
    post.method = HttpMethod::Post;
    post.body = b"payload".to_vec();
    assert_eq!(client.execute(post).unwrap().status, 302);
    let received = server.join().unwrap();
    assert!(String::from_utf8_lossy(&received).starts_with("POST "));
}

fn request_path(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned()
}

#[test]
fn get_follows_ten_redirects_and_rejects_an_eleventh() {
    fn run(final_hop: usize) -> (Result<HttpResponse, b9::transport::HttpClientError>, usize) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let accepted = Arc::new(AtomicUsize::new(0));
        let server_count = accepted.clone();
        let server = thread::spawn(move || {
            for _ in 0..=10 {
                let (mut stream, _) = listener.accept().unwrap();
                server_count.fetch_add(1, Ordering::SeqCst);
                let mut bytes = vec![0; 2048];
                let length = stream.read(&mut bytes).unwrap();
                let index: usize = request_path(&bytes[..length])
                    .trim_start_matches('/')
                    .parse()
                    .unwrap();
                if index == final_hop {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                        )
                        .unwrap();
                    break;
                }
                let response = format!(
                    "HTTP/1.1 302 Found\r\nLocation: /{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    index + 1
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        let result = HttpClient::production()
            .unwrap()
            .execute(request(format!("http://{address}/0")));
        server.join().unwrap();
        (result, accepted.load(Ordering::SeqCst))
    }

    let (result, count) = run(10);
    assert_eq!(result.unwrap().body, b"ok");
    assert_eq!(count, 11);

    let (result, count) = run(11);
    assert!(result.is_err());
    assert_eq!(count, 11);
}

#[test]
fn cross_origin_redirect_strips_sensitive_headers() {
    let destination = TcpListener::bind("127.0.0.1:0").unwrap();
    let destination_address = destination.local_addr().unwrap();
    let destination_server = thread::spawn(move || {
        let (mut stream, _) = destination.accept().unwrap();
        let mut bytes = vec![0; 4096];
        let length = stream.read(&mut bytes).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .unwrap();
        bytes.truncate(length);
        bytes
    });
    let source = TcpListener::bind("127.0.0.1:0").unwrap();
    let source_address = source.local_addr().unwrap();
    let source_server = thread::spawn(move || {
        let (mut stream, _) = source.accept().unwrap();
        let mut bytes = [0; 4096];
        let _ = stream.read(&mut bytes).unwrap();
        let response = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{destination_address}/next\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    let client = HttpClient::production().unwrap();
    let mut redirected = request(format!("http://{source_address}/start"));
    redirected.headers.push(HttpHeader {
        name: "Authorization".into(),
        value: "Bearer private".into(),
    });
    redirected.headers.push(HttpHeader {
        name: "Cookie".into(),
        value: "session=private".into(),
    });
    assert_eq!(client.execute(redirected).unwrap().body, b"ok");
    source_server.join().unwrap();
    let received = String::from_utf8_lossy(&destination_server.join().unwrap()).to_lowercase();
    assert!(!received.contains("authorization:") && !received.contains("cookie:"));
}

#[test]
fn total_timeout_is_contextual() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 1024];
        let _ = stream.read(&mut bytes).unwrap();
        thread::sleep(Duration::from_millis(150));
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    });
    let client = HttpClient::production().unwrap();
    let mut timed = request(format!("http://{address}/slow"));
    timed.timeout = Duration::from_millis(30);
    let error = client.execute(timed).unwrap_err().to_string();
    assert!(error.contains("timeout") && error.contains("retry"));
    server.join().unwrap();
}

#[test]
fn redirect_loops_fail_contextually() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = vec![0; 2048];
            let length = stream.read(&mut bytes).unwrap();
            let next = if request_path(&bytes[..length]) == "/a" {
                "/b"
            } else {
                "/a"
            };
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: {next}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    let error = HttpClient::production()
        .unwrap()
        .execute(request(format!("http://{address}/a")))
        .unwrap_err()
        .to_string();
    assert!(error.contains("loop") && error.contains("verify"));
    server.join().unwrap();
}

#[test]
fn rejected_automated_providers_have_no_reachable_acquisition_path() {
    let providers = include_str!("../src/providers/mod.rs").to_ascii_lowercase();
    let savant = include_str!("../src/providers/savant.rs").to_ascii_lowercase();
    let commands = include_str!("../src/cli.rs").to_ascii_lowercase();
    let synchronization = include_str!("../src/sync.rs").to_ascii_lowercase();
    let credentials = include_str!("../src/advisory_credentials.rs").to_ascii_lowercase();
    assert!(providers.contains("savant"));
    assert!(synchronization.contains("savantclient"));
    assert!(savant.contains("httpclient") && savant.contains(".execute("));
    assert!(!commands.contains("savant"));
    assert!(!credentials.contains("savant"));
    for rejected in ["fangraphs", "fantasypros", "rotowire"] {
        assert!(!providers.contains(rejected));
        assert!(!commands.contains(rejected));
        assert!(!synchronization.contains(rejected));
        assert!(!credentials.contains(rejected));
    }
    let policy = include_str!("../docs/skout-providers-storage.md");
    for official in [
        "mlb.com",
        "fangraphs.com",
        "fantasypros.com",
        "rotowire.com",
    ] {
        assert!(policy.contains(official));
    }
}
