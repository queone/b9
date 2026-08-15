use std::collections::VecDeque;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use b9::cache::DiskCache;
use b9::providers::mlb::{MlbClient, MlbEndpoints, PrimaryType, ScheduleCacheStatus};
use b9::store::Clock;
use b9::transport::{
    ExecutorError, HttpClient, HttpExecutor, HttpMethod, HttpResponse, ValidatedRequest,
};
use tempfile::tempdir;

const SEASON: &[u8] = include_bytes!("fixtures/mlb/season.json");
const SCHEDULE: &[u8] = include_bytes!("fixtures/mlb/schedule.json");
const BOXSCORE: &[u8] = include_bytes!("fixtures/mlb/boxscore.json");
const STANDINGS: &[u8] = include_bytes!("fixtures/mlb/standings.json");
const ROSTER: &[u8] = include_bytes!("fixtures/mlb/roster.json");
const PEOPLE: &[u8] = include_bytes!("fixtures/mlb/people.json");

// Fixture provenance is recorded in docs/api-mlbam.md. These scrubbed fixture
// shapes contain no credentials or personal operator data.

struct QueueExecutor {
    responses: Mutex<VecDeque<Result<HttpResponse, ExecutorError>>>,
    requests: Mutex<Vec<ValidatedRequest>>,
    before_response: Option<Box<dyn Fn() + Send + Sync>>,
}

impl QueueExecutor {
    fn new(responses: Vec<Result<HttpResponse, ExecutorError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
            before_response: None,
        }
    }

    fn with_hook(
        responses: Vec<Result<HttpResponse, ExecutorError>>,
        hook: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
            before_response: Some(Box::new(hook)),
        }
    }
}

impl HttpExecutor for QueueExecutor {
    fn execute(&self, request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
        self.requests.lock().unwrap().push(request);
        if let Some(hook) = &self.before_response {
            hook();
        }
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("queued MLB response")
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
        body: b"secret provider detail".to_vec(),
    })
}

fn make_client(executor: Arc<QueueExecutor>) -> MlbClient {
    MlbClient::new(
        Arc::new(HttpClient::new(executor)),
        MlbEndpoints::new("http://127.0.0.1:12345/api/v1/").unwrap(),
    )
}

#[derive(Clone)]
struct AdjustableClock(Arc<Mutex<u64>>);

impl AdjustableClock {
    fn at(seconds: u64) -> Self {
        Self(Arc::new(Mutex::new(seconds)))
    }

    fn set(&self, seconds: u64) {
        *self.0.lock().unwrap() = seconds;
    }
}

impl Clock for AdjustableClock {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(*self.0.lock().unwrap())
    }
}

#[test]
fn selected_fixtures_decode_every_typed_contract() {
    let executor = Arc::new(QueueExecutor::new(vec![
        response(SEASON),
        response(SCHEDULE),
        response(BOXSCORE),
        response(STANDINGS),
        response(ROSTER),
        response(PEOPLE),
    ]));
    let client = make_client(executor.clone());

    let season = client.fetch_season_dates(2026).unwrap();
    assert_eq!(season.season_id, "2026");
    assert_eq!(season.regular_start, "2026-03-25");
    assert_eq!(season.regular_end, "2026-09-27");
    assert_eq!(season.spring_start, "2026-02-20");
    assert_eq!(season.spring_end, "2026-03-24");

    let games = client.fetch_schedule("2026-05-15").unwrap();
    assert_eq!(games.len(), 3);
    assert_eq!(games[0].game_id, 800001);
    assert_eq!(games[0].game_date, "2026-05-15T23:05:00Z");
    assert_eq!(games[0].detailed_state, "In Progress");
    assert_eq!(games[0].away_team_id, 110);
    assert_eq!(games[0].away_team_name, "Baltimore Orioles");
    assert_eq!(games[0].home_team_id, 147);
    assert_eq!(games[0].home_team_name, "New York Yankees");
    assert_eq!(games[0].away_probable_pitcher_id, Some(600001));
    assert_eq!(games[0].away_probable_pitcher_name, "Away Starter");
    assert_eq!(games[0].home_probable_pitcher_id, Some(600002));
    assert_eq!(games[0].home_probable_pitcher_name, "Home Starter");
    assert_eq!(games[0].linescore.as_ref().unwrap().inning, Some(5));
    assert_eq!(games[0].linescore.as_ref().unwrap().inning_ordinal, "5th");
    assert_eq!(games[0].linescore.as_ref().unwrap().inning_state, "Top");
    assert_eq!(games[0].linescore.as_ref().unwrap().away_runs, 2);
    assert_eq!(games[0].linescore.as_ref().unwrap().home_runs, 1);
    assert_eq!(
        games[0].away_lineup.as_ref().unwrap(),
        &[b9::providers::mlb::LineupPlayer {
            person_id: 700001,
            full_name: "Away Hitter".into(),
        }]
    );
    assert_eq!(
        games[0].home_lineup.as_ref().unwrap(),
        &[b9::providers::mlb::LineupPlayer {
            person_id: 700002,
            full_name: "Home Hitter".into(),
        }]
    );
    assert!(games[1].linescore.is_none());
    assert_eq!(games[1].away_probable_pitcher_id, None);
    assert_eq!(games[1].home_probable_pitcher_id, None);
    assert_eq!(games[2].detailed_state, "Final");
    assert_eq!(games[2].linescore.as_ref().unwrap().inning, Some(9));
    assert_eq!(games[2].linescore.as_ref().unwrap().inning_state, "End");
    assert_eq!(games[2].linescore.as_ref().unwrap().away_runs, 4);
    assert_eq!(games[2].linescore.as_ref().unwrap().home_runs, 3);

    let boxscore = client.fetch_boxscore(800001).unwrap();
    assert_eq!(boxscore.away.batting_order, vec![700001]);
    assert_eq!(boxscore.away.bench, vec![700003]);
    assert_eq!(boxscore.away.players.len(), 1);
    let hitter = &boxscore.away.players[&700001];
    assert_eq!(hitter.person_id, 700001);
    assert_eq!(hitter.full_name, "Away Hitter");
    assert_eq!(hitter.batting.as_ref().unwrap().hits, Some(2));
    assert_eq!(hitter.batting.as_ref().unwrap().at_bats, Some(3));
    assert_eq!(hitter.batting.as_ref().unwrap().runs, Some(1));
    assert_eq!(hitter.batting.as_ref().unwrap().home_runs, Some(1));
    assert_eq!(hitter.batting.as_ref().unwrap().rbi, Some(2));
    assert_eq!(hitter.batting.as_ref().unwrap().stolen_bases, Some(0));
    assert!(hitter.pitching.is_none());
    let pitcher = &boxscore.home.players[&600002];
    assert_eq!(pitcher.person_id, 600002);
    assert_eq!(pitcher.full_name, "Home Pitcher");
    assert!(pitcher.batting.is_none());
    assert_eq!(
        pitcher
            .pitching
            .as_ref()
            .unwrap()
            .innings_pitched
            .as_deref(),
        Some("5.1")
    );
    assert_eq!(pitcher.pitching.as_ref().unwrap().walks, Some(2));
    assert_eq!(pitcher.pitching.as_ref().unwrap().wins, Some(0));
    assert_eq!(pitcher.pitching.as_ref().unwrap().saves, Some(0));
    assert_eq!(pitcher.pitching.as_ref().unwrap().strikeouts, Some(7));
    assert_eq!(
        pitcher.pitching.as_ref().unwrap().era.as_deref(),
        Some("3.14")
    );
    assert_eq!(
        pitcher.pitching.as_ref().unwrap().whip.as_deref(),
        Some("1.10")
    );
    assert_eq!(pitcher.pitching.as_ref().unwrap().earned_runs, Some(2));
    assert_eq!(pitcher.pitching.as_ref().unwrap().hits_allowed, Some(4));

    let standings = client.fetch_standings(2026).unwrap();
    assert_eq!(standings.len(), 2);
    assert_eq!(standings[0].team_id, 147);
    assert_eq!(standings[0].wins, 30);
    assert_eq!(standings[0].losses, 20);
    assert_eq!(standings[0].games_back, "—");
    assert_eq!(standings[1].team_id, 119);
    assert_eq!(standings[1].wins, 31);
    assert_eq!(standings[1].losses, 19);
    assert_eq!(standings[1].games_back, "—");

    let roster = client.fetch_roster(119).unwrap();
    assert_eq!(roster.len(), 4);
    assert_eq!(roster[0].primary_type, PrimaryType::H);
    assert_eq!(roster[0].person_id, 660271);
    assert_eq!(roster[0].full_name, "Two Way");
    assert_eq!(roster[0].position, "TWP");
    assert_eq!(roster[1].primary_type, PrimaryType::P);
    assert_eq!(roster[1].person_id, roster[0].person_id);
    assert_eq!(roster[1].full_name, roster[0].full_name);
    assert_eq!(roster[1].position, roster[0].position);
    assert_eq!(roster[1].status, roster[0].status);
    assert_eq!(roster[1].jersey_number, roster[0].jersey_number);
    assert_eq!(roster[0].status, "A");
    assert_eq!(roster[0].jersey_number, "17");
    assert_eq!(roster[2].position, "SP");
    assert_eq!(roster[2].status, "A");
    assert_eq!(roster[3].primary_type, PrimaryType::H);

    let people = client.fetch_people(&[699009, 699008]).unwrap();
    assert_eq!(people.len(), 2);
    assert_eq!(people[0].person_id, 699009);
    assert_eq!(people[0].full_name, "Free Agent");
    assert_eq!(people[0].primary_position, "P");
    assert_eq!(people[0].bat_side, "");
    assert_eq!(people[0].pitch_hand, "L");
    assert_eq!(people[0].current_team, "");
    assert_eq!(people[0].birth_date, None);
    assert_eq!(people[1].birth_date.as_deref(), Some("2002-01-02"));
    assert_eq!(people[1].full_name, "Prospect One");
    assert_eq!(people[1].primary_position, "SS");
    assert_eq!(people[1].bat_side, "R");
    assert_eq!(people[1].pitch_hand, "R");
    assert_eq!(people[1].current_team, "NYY");

    let requests = executor.requests.lock().unwrap();
    assert_eq!(requests.len(), 6);
    for request in requests.iter() {
        assert_eq!(request.method(), HttpMethod::Get);
        assert_eq!(request.timeout(), Duration::from_secs(10));
        assert_eq!(request.body_limit(), 8 * 1024 * 1024);
    }
    assert!(requests[0].url().contains("/seasons/2026?sportId=1"));
    assert!(requests[1].url().contains("sportId=1&date=2026-05-15"));
    assert!(
        requests[1]
            .url()
            .contains("hydrate=linescore%2CprobablePitcher%2Clineups")
    );
    assert!(requests[2].url().contains("/game/800001/boxscore"));
    assert!(requests[3].url().contains("leagueId=103%2C104&season=2026"));
    assert!(requests[4].url().contains("rosterType=40Man"));
    assert!(requests[5].url().contains("personIds=699009%2C699008"));
}

#[test]
fn validation_and_empty_people_precede_all_side_effects() {
    let executor = Arc::new(QueueExecutor::new(Vec::new()));
    let client = make_client(executor.clone());
    for season in [1875, 10000] {
        assert!(client.fetch_season_dates(season).is_err());
        assert!(client.fetch_standings(season).is_err());
    }
    for date in ["2026-2-01", "2026-02-29", "2026-13-01", "abcd-ef-gh"] {
        assert!(client.fetch_schedule(date).is_err());
        let directory = tempdir().unwrap();
        assert!(
            client
                .fetch_schedule_cached(date, &DiskCache::at(directory.path()))
                .is_err()
        );
        assert!(!directory.path().join("mlb").exists());
    }
    assert!(client.fetch_boxscore(0).is_err());
    assert!(client.fetch_roster(-1).is_err());
    assert!(client.fetch_people(&[1, 0]).is_err());
    assert!(client.fetch_people(&[]).unwrap().is_empty());
    assert!(executor.requests.lock().unwrap().is_empty());
}

#[test]
fn people_are_deduplicated_batched_ordered_and_atomic() {
    let first = br#"{"people":[{"id":2,"fullName":"Two"},{"id":1,"fullName":"One"},{"id":999,"fullName":"No"}]}"#;
    let second = br#"{"people":[{"id":101,"fullName":"Last"}]}"#;
    let executor = Arc::new(QueueExecutor::new(vec![response(first), response(second)]));
    let client = make_client(executor.clone());
    let mut ids: Vec<i64> = (1..=101).collect();
    ids.push(2);
    let people = client.fetch_people(&ids).unwrap();
    assert_eq!(
        people
            .iter()
            .map(|person| person.person_id)
            .collect::<Vec<_>>(),
        vec![1, 2, 101]
    );
    let requests = executor.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].url().contains("personIds=1%2C2%2C3"));
    assert!(requests[1].url().contains("personIds=101"));
    drop(requests);

    let executor = Arc::new(QueueExecutor::new(vec![response(first), status(503)]));
    let error = make_client(executor.clone())
        .fetch_people(&ids)
        .unwrap_err();
    assert_eq!(error.operation_name(), "fetch MLB people identities");
    assert!(!error.to_string().contains("secret provider detail"));
    assert_eq!(executor.requests.lock().unwrap().len(), 2);
}

#[test]
fn required_envelopes_and_failures_never_become_empty_success() {
    for (body, operation) in [
        (br#"{}"#.as_slice(), "fetch MLB season dates"),
        (br#"{"seasons":[]}"#.as_slice(), "fetch MLB season dates"),
        (br#"{}"#.as_slice(), "fetch MLB schedule"),
        (br#"{}"#.as_slice(), "fetch MLB boxscore"),
        (br#"{}"#.as_slice(), "fetch MLB standings"),
        (br#"{}"#.as_slice(), "fetch MLB 40-man roster"),
        (br#"{}"#.as_slice(), "fetch MLB people identities"),
    ] {
        let executor = Arc::new(QueueExecutor::new(vec![response(body)]));
        let client = make_client(executor);
        let error = match operation {
            "fetch MLB season dates" => client.fetch_season_dates(2026).unwrap_err(),
            "fetch MLB schedule" => client.fetch_schedule("2026-05-15").unwrap_err(),
            "fetch MLB boxscore" => client.fetch_boxscore(1).unwrap_err(),
            "fetch MLB standings" => client.fetch_standings(2026).unwrap_err(),
            "fetch MLB 40-man roster" => client.fetch_roster(1).unwrap_err(),
            _ => client.fetch_people(&[1]).unwrap_err(),
        };
        assert_eq!(error.operation_name(), operation);
    }
}

#[test]
fn common_transport_status_size_and_json_failures_are_contextual() {
    let failures = vec![
        status(429),
        response(br#"{"dates":["#),
        Err(ExecutorError::ResponseTooLarge { limit: 8 }),
        Err(ExecutorError::Dispatch {
            detail: "offline".into(),
            source: None,
        }),
    ];
    for failure in failures {
        let executor = Arc::new(QueueExecutor::new(vec![failure]));
        let error = make_client(executor.clone())
            .fetch_schedule("2026-05-15")
            .unwrap_err();
        assert_eq!(error.operation_name(), "fetch MLB schedule");
        assert!(!error.to_string().contains("secret provider detail"));
        assert_eq!(executor.requests.lock().unwrap().len(), 1);
    }
}

#[test]
fn schedule_cache_honors_hit_expiry_corruption_and_write_degradation() {
    let directory = tempdir().unwrap();
    let clock = AdjustableClock::at(100);
    let cache = DiskCache::at_with_clock(directory.path(), Arc::new(clock.clone()));
    let executor = Arc::new(QueueExecutor::new(vec![
        response(SCHEDULE),
        response(SCHEDULE),
    ]));
    let client = make_client(executor.clone());

    let first = client.fetch_schedule_cached("2026-05-15", &cache).unwrap();
    assert_eq!(first.cache_status, ScheduleCacheStatus::Miss);
    assert!(first.cache_write_issue.is_none());
    let hit = client.fetch_schedule_cached("2026-05-15", &cache).unwrap();
    assert_eq!(hit.cache_status, ScheduleCacheStatus::Hit);
    assert_eq!(executor.requests.lock().unwrap().len(), 1);
    clock.set(160);
    let expired = client.fetch_schedule_cached("2026-05-15", &cache).unwrap();
    assert_eq!(expired.cache_status, ScheduleCacheStatus::Expired);
    assert_eq!(executor.requests.lock().unwrap().len(), 2);

    cache
        .put("mlb", "schedule-2026-05-16", b"not json")
        .unwrap();
    let executor = Arc::new(QueueExecutor::new(vec![response(SCHEDULE)]));
    let corrupt = make_client(executor)
        .fetch_schedule_cached("2026-05-16", &cache)
        .unwrap();
    assert_eq!(corrupt.cache_status, ScheduleCacheStatus::Corrupt);

    cache.put("mlb", "schedule-2026-05-17", SCHEDULE).unwrap();
    let path = cache.entry_path("mlb", "schedule-2026-05-17").unwrap();
    fs::write(path, b"broken frame").unwrap();
    let executor = Arc::new(QueueExecutor::new(vec![response(SCHEDULE)]));
    let corrupt = make_client(executor)
        .fetch_schedule_cached("2026-05-17", &cache)
        .unwrap();
    assert_eq!(corrupt.cache_status, ScheduleCacheStatus::Corrupt);

    let failure_root = tempdir().unwrap();
    let cache = DiskCache::at(failure_root.path());
    let namespace = failure_root.path().join("mlb");
    let executor = Arc::new(QueueExecutor::with_hook(
        vec![response(SCHEDULE)],
        move || {
            if !namespace.exists() {
                fs::write(&namespace, b"block directory creation").unwrap();
            }
        },
    ));
    let degraded = make_client(executor)
        .fetch_schedule_cached("2026-05-15", &cache)
        .unwrap();
    assert_eq!(degraded.cache_status, ScheduleCacheStatus::Miss);
    assert!(degraded.cache_write_issue.is_some());
    assert!(degraded.cache_write_issue.unwrap().chars().count() <= 256);
}

#[test]
fn cache_read_failures_and_endpoint_errors_are_contextual() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("mlb"), b"not a directory").unwrap();
    let cache = DiskCache::at(directory.path());
    let executor = Arc::new(QueueExecutor::new(Vec::new()));
    let error = make_client(executor.clone())
        .fetch_schedule_cached("2026-05-15", &cache)
        .unwrap_err();
    assert_eq!(error.operation_name(), "fetch cached MLB schedule");
    assert!(executor.requests.lock().unwrap().is_empty());

    for root in [
        "http://example.com/api/v1/",
        "https://user:pass@example.com/api/v1/",
        "https://example.com/api/v1/?token=x",
    ] {
        assert!(MlbEndpoints::new(root).is_err());
    }
}

#[test]
fn provider_source_has_no_database_or_persistence_dependency() {
    let source = include_str!("../src/providers/mlb.rs");
    assert!(!source.contains("rusqlite"));
    assert!(!source.contains("crate::store"));
    assert!(!source.contains("Store"));
}
