use std::sync::{Arc, Mutex};

use b9::providers::oddsshark::{OddsSharkClient, OddsSharkEndpoints};
use b9::transport::{ExecutorError, HttpClient, HttpExecutor, HttpResponse, ValidatedRequest};

struct Executor {
    request: Mutex<Option<ValidatedRequest>>,
}
impl HttpExecutor for Executor {
    fn execute(&self, request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
        *self.request.lock().unwrap() = Some(request);
        Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: include_bytes!("fixtures/oddsshark/slate.json").to_vec(),
        })
    }
}

#[test]
fn future_lines_require_referer_and_retain_valid_shape_variants() {
    let executor = Arc::new(Executor {
        request: Mutex::new(None),
    });
    let client = OddsSharkClient::new(
        Arc::new(HttpClient::new(executor.clone())),
        OddsSharkEndpoints::new("http://127.0.0.1:12345/api/scores/mlb").unwrap(),
    );
    let rows = client.fetch_game_lines("2026-08-16").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].away_team, "Boston Red Sox");
    assert_eq!(rows[1].home_moneyline, 100);
    let request = executor.request.lock().unwrap();
    let request = request.as_ref().unwrap();
    assert!(request.url().ends_with("api/scores/mlb?date=2026-08-16"));
    assert!(
        request
            .headers()
            .iter()
            .any(|header| header.name.eq_ignore_ascii_case("referer")
                && header.value == "https://www.oddsshark.com/mlb/scores")
    );
    assert_eq!(request.body_limit(), 4 * 1024 * 1024);
}

#[test]
fn invalid_dates_fail_before_transport() {
    let executor = Arc::new(Executor {
        request: Mutex::new(None),
    });
    let client = OddsSharkClient::new(
        Arc::new(HttpClient::new(executor.clone())),
        OddsSharkEndpoints::new("http://127.0.0.1:12345/api/scores/mlb").unwrap(),
    );
    assert!(client.fetch_game_lines("tomorrow").is_err());
    assert!(executor.request.lock().unwrap().is_none());
}
