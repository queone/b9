use std::sync::{Arc, Mutex};

use b9::fetch_command::run_with_client;
use b9::providers::{fangraphs::FangraphsClient, fangraphs_closer_chart, fantasypros};
use b9::transport::{
    ExecutorError, HttpClient, HttpExecutor, HttpHeader, HttpResponse, ValidatedRequest,
};

struct RecordingExecutor {
    response: HttpResponse,
    requests: Mutex<Vec<ValidatedRequest>>,
}

impl HttpExecutor for RecordingExecutor {
    fn execute(&self, request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
        self.requests.lock().unwrap().push(request);
        Ok(self.response.clone())
    }
}

fn client(body: &str) -> (Arc<HttpClient>, Arc<RecordingExecutor>) {
    let executor = Arc::new(RecordingExecutor {
        response: HttpResponse {
            status: 200,
            headers: vec![HttpHeader {
                name: "x-test".into(),
                value: "yes".into(),
            }],
            body: body.as_bytes().to_vec(),
        },
        requests: Mutex::new(Vec::new()),
    });
    (Arc::new(HttpClient::new(executor.clone())), executor)
}

#[test]
fn fangraphs_typed_json_accepts_projection_string_ids_and_batted_ball_fields() {
    let (http, _) = client(r#"{"data":[{"playerid":7,"xMLBAMID":42,"FB%":0.4,"HR/FB":0.2}]}"#);
    let leaders = FangraphsClient::new(http)
        .fetch_json::<b9::providers::fangraphs::LeaderRow>("https://www.fangraphs.com/test")
        .unwrap();
    assert_eq!(
        (leaders[0].mlbam_id, leaders[0].fb_pct, leaders[0].hr_fb_pct),
        (Some(42), 0.4, 0.2)
    );
    let (http, _) = client(r#"[{"playerid":"7","PA":600,"HR":30}]"#);
    let rows = FangraphsClient::new(http)
        .fetch_json::<b9::providers::fangraphs::ProjectionRow>("https://www.fangraphs.com/test")
        .unwrap();
    assert_eq!(
        (rows[0].fangraphs_id, rows[0].pa, rows[0].hr),
        (7, 600.0, 30.0)
    );
}

#[test]
fn html_parsers_handle_rows_and_page_shape_drift() {
    let chart = fangraphs_closer_chart::parse_html(r#"<tr><td data-stat="TEAM">WSN</td><td data-stat="PLAYER"><b>Ace Arm</b></td><td data-stat="PROJECTED ROLE">Closer</td></tr>"#).unwrap();
    assert_eq!(
        (chart[0].team.as_str(), chart[0].name.as_str()),
        ("WSH", "Ace Arm")
    );
    let rows = fantasypros::parse_html(r#"<script>var ecrData = {"players":[{"player_name":"Ada Ace","player_team_id":"NYY","yahoo_player_id":"9","rank_ecr":3}]};</script>"#).unwrap();
    assert_eq!((rows[0].yahoo_player_id, rows[0].rank), (Some(9), 3));
    assert!(
        fantasypros::parse_html("<html>changed</html>")
            .unwrap_err()
            .to_string()
            .contains("marker")
    );
}

#[test]
fn fetch_is_origin_pinned_and_preserves_raw_response_and_provider_headers() {
    let (http, executor) = client("raw body");
    let output = run_with_client(&http, "fangraphs", "/closers").unwrap();
    assert_eq!(output, "HTTP 200\nx-test: yes\n\nraw body");
    let requests = executor.requests.lock().unwrap();
    assert_eq!(requests[0].url(), "https://www.fangraphs.com/closers");
    assert!(
        requests[0]
            .headers()
            .iter()
            .any(|header| header.name.eq_ignore_ascii_case("accept"))
    );
    assert!(run_with_client(&http, "mlb", "https://evil.test").is_err());
    assert!(run_with_client(&http, "mlb", "//evil.test").is_err());
}
