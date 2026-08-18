use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::Mutex;

use b9::config::Config;
use b9::providers::yahoo_public::YahooPublicClient;
use b9::public_pull::pull_with;
use b9::store::{Store, SyncMode, SyncOrigin};
use b9::transport::{ExecutorError, HttpClient, HttpExecutor, HttpResponse, ValidatedRequest};
use tempfile::tempdir;

const REDZONE_VALID: &[u8] = include_bytes!("fixtures/yahoo-public/redzone_valid.json");
const PUBLIC_RANKS: &[u8] = br#"{"fantasy_content":{"league":{"players":[{"player":{"player_id":"10395","player_ranks":[{"player_rank":{"rank_type":"S","rank_value":"216","rank_season":"2026"}},{"player_rank":{"rank_type":"S","rank_position":"C","rank_value":"12","rank_season":"2026"}}]}}]}}}"#;

struct FixedExecutor {
    responses: Mutex<VecDeque<Result<HttpResponse, ExecutorError>>>,
}

impl FixedExecutor {
    fn new(responses: Vec<Result<HttpResponse, ExecutorError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
        }
    }
}

impl HttpExecutor for FixedExecutor {
    fn execute(&self, _request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("fixed response available")
    }
}

fn ok_client() -> YahooPublicClient {
    YahooPublicClient::new(HttpClient::new(Arc::new(FixedExecutor::new(vec![
        Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: REDZONE_VALID.to_vec(),
        }),
        Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: PUBLIC_RANKS.to_vec(),
        }),
    ]))))
}

fn failing_client() -> YahooPublicClient {
    YahooPublicClient::new(HttpClient::new(Arc::new(FixedExecutor::new(vec![Ok(
        HttpResponse {
            status: 403,
            headers: Vec::new(),
            body: Vec::new(),
        },
    )]))))
}

#[test]
fn successful_pull_writes_the_snapshot_and_records_a_complete_public_pull_run() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
    let mut config = Config {
        pull_public_league_id: "170874".into(),
        ..Config::default()
    };
    let client = ok_client();

    let output = pull_with(
        &client,
        &mut store,
        &mut config,
        None,
        false,
        &mut Cursor::new(""),
        &mut Vec::new(),
    )
    .unwrap();
    assert!(output.contains("2 teams"));
    assert!(output.contains("5 players"));

    assert_eq!(config.current_league, "public.170874");
    assert_eq!(store.fantasy_teams("public.170874").unwrap().len(), 2);
    assert_eq!(
        store
            .fantasy_players("public.170874")
            .unwrap()
            .iter()
            .find(|player| player.yahoo_player_id == Some(10395))
            .and_then(|player| player.rank),
        Some(216)
    );
    assert_eq!(
        store.current_data_origin(SyncMode::Live).unwrap(),
        Some(SyncOrigin::PublicPull)
    );
}

#[test]
fn failed_fetch_retains_prior_snapshot_and_leaves_origin_unchanged() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
    let mut config = Config {
        pull_public_league_id: "170874".into(),
        ..Config::default()
    };

    pull_with(
        &ok_client(),
        &mut store,
        &mut config,
        None,
        false,
        &mut Cursor::new(""),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(
        store.current_data_origin(SyncMode::Live).unwrap(),
        Some(SyncOrigin::PublicPull)
    );

    let error = pull_with(
        &failing_client(),
        &mut store,
        &mut config,
        None,
        false,
        &mut Cursor::new(""),
        &mut Vec::new(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("403"));

    // A failed run never changes what "current data origin" reports, and
    // the prior successful snapshot is untouched.
    assert_eq!(
        store.current_data_origin(SyncMode::Live).unwrap(),
        Some(SyncOrigin::PublicPull)
    );
    assert_eq!(store.fantasy_teams("public.170874").unwrap().len(), 2);
}

#[test]
fn a_later_official_sync_takes_precedence_over_a_prior_public_pull() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
    let mut config = Config {
        pull_public_league_id: "170874".into(),
        ..Config::default()
    };

    pull_with(
        &ok_client(),
        &mut store,
        &mut config,
        None,
        false,
        &mut Cursor::new(""),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(
        store.current_data_origin(SyncMode::Live).unwrap(),
        Some(SyncOrigin::PublicPull)
    );

    // Simulate a subsequent official OAuth sync completing.
    let run = store
        .start_sync_run(SyncMode::Live, SyncOrigin::Manual)
        .unwrap();
    store
        .complete_sync_run(run, &std::collections::BTreeMap::new())
        .unwrap();
    assert_eq!(
        store.current_data_origin(SyncMode::Live).unwrap(),
        Some(SyncOrigin::Manual)
    );
}

#[test]
fn a_prior_logins_team_selection_survives_a_pull_that_reuses_the_real_league_key() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
    // Simulates a config already populated by a real `b9 login` + `b9 sync`.
    let mut config = Config {
        current_league: "469.l.170874".into(),
        current_team_key: "469.l.170874.t.1".into(),
        ..Config::default()
    };

    pull_with(
        &ok_client(),
        &mut store,
        &mut config,
        None,
        false,
        &mut Cursor::new(""),
        &mut Vec::new(),
    )
    .unwrap();

    // The real key is reused for storage, and "my team" is never cleared —
    // existing commands that resolve the operator's own team keep working.
    assert_eq!(config.current_league, "469.l.170874");
    assert_eq!(config.current_team_key, "469.l.170874.t.1");
    assert!(
        store
            .fantasy_teams("469.l.170874")
            .unwrap()
            .iter()
            .any(|team| team.team_key == "469.l.170874.t.1")
    );
}

#[test]
fn interactive_prompt_resolves_and_persists_when_nothing_else_is_configured() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
    let mut config = Config::default();
    let mut output = Vec::new();

    pull_with(
        &ok_client(),
        &mut store,
        &mut config,
        None,
        true,
        &mut Cursor::new("170874\n"),
        &mut output,
    )
    .unwrap();

    assert_eq!(config.pull_public_league_id, "170874");
    assert_eq!(config.current_league, "public.170874");
    assert!(String::from_utf8(output).unwrap().contains("League id:"));
}
