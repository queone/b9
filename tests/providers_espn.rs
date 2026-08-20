use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use skout::providers::espn::{EspnClient, EspnEndpoints};
use skout::transport::{ExecutorError, HttpClient, HttpExecutor, HttpResponse, ValidatedRequest};

const SCOREBOARD_STANDARD: &[u8] = include_bytes!("fixtures/espn/scoreboard-standard.json");
const SCOREBOARD_EMPTY: &[u8] = include_bytes!("fixtures/espn/scoreboard-empty.json");
const SCOREBOARD_MALFORMED: &[u8] = include_bytes!("fixtures/espn/scoreboard-malformed.json");
const SCOREBOARD_INCOMPLETE: &[u8] = include_bytes!("fixtures/espn/scoreboard-incomplete.json");
const ODDS_STANDARD: &[u8] = include_bytes!("fixtures/espn/odds-standard.json");
const ODDS_EMPTY: &[u8] = include_bytes!("fixtures/espn/odds-empty.json");
const ODDS_ZERO: &[u8] = include_bytes!("fixtures/espn/odds-zero-moneyline.json");
const ODDS_MALFORMED: &[u8] = include_bytes!("fixtures/espn/odds-malformed.json");

// Fixture provenance: captured shapes from the two ESPN endpoints documented in
// docs/api-espn.md and scrubbed on 2026-08-15. No personal or credential data.

struct QueueExecutor {
    responses: Mutex<VecDeque<Result<HttpResponse, ExecutorError>>>,
    requests: Mutex<Vec<ValidatedRequest>>,
}

impl QueueExecutor {
    fn new(responses: Vec<Result<HttpResponse, ExecutorError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        }
    }
}

impl HttpExecutor for QueueExecutor {
    fn execute(&self, request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
        self.requests.lock().unwrap().push(request);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("queued response")
    }
}

struct HeaderRequiredExecutor {
    requests: Mutex<Vec<ValidatedRequest>>,
}

impl HttpExecutor for HeaderRequiredExecutor {
    fn execute(&self, request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
        let accepted = request.headers().iter().any(|header| {
            header.name.eq_ignore_ascii_case("user-agent")
                && header.value
                    == format!(
                        "skout/{} (+https://github.com/queone/skout)",
                        env!("CARGO_PKG_VERSION")
                    )
        });
        self.requests.lock().unwrap().push(request);
        Ok(HttpResponse {
            status: if accepted { 200 } else { 403 },
            headers: Vec::new(),
            body: if accepted {
                SCOREBOARD_EMPTY.to_vec()
            } else {
                b"header required".to_vec()
            },
        })
    }
}

fn response(body: &[u8]) -> Result<HttpResponse, ExecutorError> {
    Ok(HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: body.to_vec(),
    })
}

fn status(code: u16) -> Result<HttpResponse, ExecutorError> {
    Ok(HttpResponse {
        status: code,
        headers: Vec::new(),
        body: b"provider detail".to_vec(),
    })
}

fn client(responses: Vec<Result<HttpResponse, ExecutorError>>) -> (EspnClient, Arc<QueueExecutor>) {
    let executor = Arc::new(QueueExecutor::new(responses));
    let http = Arc::new(HttpClient::new(executor.clone()));
    let endpoints = EspnEndpoints::new(
        "http://127.0.0.1:12345/scoreboard",
        "http://127.0.0.1:12345/core/",
    )
    .unwrap();
    (EspnClient::new(http, endpoints), executor)
}

fn day() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_778_803_200)
}

#[test]
fn requests_two_utc_days_deduplicates_and_uses_first_odds_item() {
    let (client, executor) = client(vec![
        response(SCOREBOARD_STANDARD),
        response(SCOREBOARD_STANDARD),
        response(ODDS_STANDARD),
        response(ODDS_EMPTY),
    ]);
    let result = client.fetch_game_lines(day()).unwrap();
    assert_eq!(result.games.len(), 2);
    assert!(result.issues.is_empty());
    assert_eq!(result.games[0].event_id, "event-1");
    assert_eq!(result.games[0].sportsbook, "Top Book");
    assert_eq!(result.games[0].home_moneyline, -180);
    assert_eq!(result.games[0].away_moneyline, 148);
    assert!(result.games[0].quoted);
    assert!(!result.games[1].quoted);

    let requests = executor.requests.lock().unwrap();
    let urls: Vec<_> = requests.iter().map(|request| request.url()).collect();
    assert!(urls[0].ends_with("scoreboard?dates=20260515"));
    assert!(urls[1].ends_with("scoreboard?dates=20260516"));
    assert!(urls[2].ends_with("/events/event-1/competitions/competition-1/odds"));
    assert_eq!(requests[0].timeout(), Duration::from_secs(10));
    assert_eq!(requests[0].body_limit(), 4 * 1024 * 1024);
    for request in requests.iter() {
        let headers = request.headers();
        assert!(headers.iter().any(|header| {
            header.name.eq_ignore_ascii_case("user-agent")
                && header.value
                    == format!(
                        "skout/{} (+https://github.com/queone/skout)",
                        env!("CARGO_PKG_VERSION")
                    )
        }));
        assert!(headers.iter().any(|header| {
            header.name.eq_ignore_ascii_case("accept") && header.value == "application/json"
        }));
    }
}

#[test]
fn provider_that_rejects_a_missing_user_agent_accepts_settled_headers() {
    let executor = Arc::new(HeaderRequiredExecutor {
        requests: Mutex::new(Vec::new()),
    });
    let http = Arc::new(HttpClient::new(executor.clone()));
    let endpoints = EspnEndpoints::new(
        "http://127.0.0.1:12345/scoreboard",
        "http://127.0.0.1:12345/core/",
    )
    .unwrap();

    let result = EspnClient::new(http, endpoints).fetch_game_lines(day());

    assert!(result.is_ok());
    assert_eq!(executor.requests.lock().unwrap().len(), 2);
}

#[test]
fn incomplete_events_are_skipped_and_empty_scoreboards_succeed() {
    let (client, executor) = client(vec![
        response(SCOREBOARD_INCOMPLETE),
        response(SCOREBOARD_EMPTY),
    ]);
    let result = client.fetch_game_lines(day()).unwrap();
    assert!(result.games.is_empty());
    assert!(result.issues.is_empty());
    assert_eq!(executor.requests.lock().unwrap().len(), 2);
}

#[test]
fn scoreboard_failures_abort_before_odds() {
    for first in [status(503), response(SCOREBOARD_MALFORMED)] {
        let (client, executor) = client(vec![first]);
        let error = client.fetch_game_lines(day()).unwrap_err();
        assert_eq!(error.operation_name(), "fetch ESPN scoreboard");
        assert_eq!(executor.requests.lock().unwrap().len(), 1);
        assert!(!error.to_string().contains("provider detail"));
    }

    let (client, executor) = client(vec![response(SCOREBOARD_EMPTY), status(502)]);
    assert!(client.fetch_game_lines(day()).is_err());
    assert_eq!(executor.requests.lock().unwrap().len(), 2);
}

#[test]
fn per_game_failures_retain_games_and_are_bounded_in_order() {
    let dispatch = ExecutorError::Dispatch {
        detail: "x".repeat(400),
        source: None,
    };
    let (client, _) = client(vec![
        response(SCOREBOARD_STANDARD),
        response(SCOREBOARD_EMPTY),
        Err(dispatch),
        response(ODDS_MALFORMED),
    ]);
    let result = client.fetch_game_lines(day()).unwrap();
    assert_eq!(result.games.len(), 2);
    assert_eq!(result.issues.len(), 2);
    assert_eq!(result.issues[0].event_id, "event-1");
    assert_eq!(result.issues[1].event_id, "event-2");
    assert!(
        result
            .issues
            .iter()
            .all(|issue| issue.detail.chars().count() <= 256)
    );
}

#[test]
fn per_game_status_and_size_failures_degrade_without_retries() {
    let (client, executor) = client(vec![
        response(SCOREBOARD_STANDARD),
        response(SCOREBOARD_EMPTY),
        status(429),
        Err(ExecutorError::ResponseTooLarge { limit: 4 }),
    ]);
    let result = client.fetch_game_lines(day()).unwrap();
    assert_eq!(result.games.len(), 2);
    assert_eq!(result.issues.len(), 2);
    assert_eq!(executor.requests.lock().unwrap().len(), 4);
}

#[test]
fn zero_moneyline_is_successfully_unquoted() {
    let (client, _) = client(vec![
        response(SCOREBOARD_STANDARD),
        response(SCOREBOARD_EMPTY),
        response(ODDS_ZERO),
        response(ODDS_EMPTY),
    ]);
    let result = client.fetch_game_lines(day()).unwrap();
    assert!(!result.games[0].quoted);
    assert_eq!(result.games[0].sportsbook, "No Line Book");
}

#[test]
fn team_mapping_ignores_spacing_and_punctuation_only() {
    assert!(skout::providers::espn::matches_team(
        "N.Y. Yankees",
        "NY Yankees"
    ));
    assert!(!skout::providers::espn::matches_team(
        "New York Mets",
        "New York Yankees"
    ));
}

#[test]
fn provider_identifiers_are_path_encoded() {
    let scoreboard = br#"{
        "events": [{
            "id": "event/one",
            "competitions": [{
                "id": "competition?one",
                "competitors": [
                    {"homeAway": "home", "team": {"displayName": "Home"}},
                    {"homeAway": "away", "team": {"displayName": "Away"}}
                ]
            }]
        }]
    }"#;
    let (client, executor) = client(vec![
        response(scoreboard),
        response(SCOREBOARD_EMPTY),
        response(ODDS_EMPTY),
    ]);
    client.fetch_game_lines(day()).unwrap();
    let requests = executor.requests.lock().unwrap();
    assert!(
        requests[2]
            .url()
            .ends_with("/events/event%2Fone/competitions/competition%3Fone/odds")
    );
}

#[test]
fn endpoint_and_day_validation_precede_dispatch() {
    for endpoint in [
        "http://example.com/scoreboard",
        "https://user:pass@example.com/scoreboard",
        "https://example.com/scoreboard?secret=value",
        "https://example.com/scoreboard#fragment",
    ] {
        assert!(EspnEndpoints::new(endpoint, "https://example.com/core/").is_err());
    }
    let (client, executor) = client(Vec::new());
    assert!(
        client
            .fetch_game_lines(UNIX_EPOCH - Duration::from_secs(1))
            .is_err()
    );
    assert!(executor.requests.lock().unwrap().is_empty());
}
