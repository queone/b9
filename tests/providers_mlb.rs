use std::collections::VecDeque;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use skout::cache::DiskCache;
use skout::providers::mlb::{
    HittingStats, MlbCacheStatus, MlbClient, MlbEndpoints, PitchingStats, PrimaryType,
};
use skout::store::Clock;
use skout::transport::{
    ExecutorError, HttpClient, HttpExecutor, HttpMethod, HttpResponse, ValidatedRequest,
};
use tempfile::tempdir;

const SEASON: &[u8] = include_bytes!("fixtures/mlb/season.json");
const SCHEDULE: &[u8] = include_bytes!("fixtures/mlb/schedule.json");
const BOXSCORE: &[u8] = include_bytes!("fixtures/mlb/boxscore.json");
const STANDINGS: &[u8] = include_bytes!("fixtures/mlb/standings.json");
const TEAM_DIRECTORY: &[u8] = include_bytes!("fixtures/mlb/team-directory.json");
const ROSTER: &[u8] = include_bytes!("fixtures/mlb/roster.json");
const PEOPLE: &[u8] = include_bytes!("fixtures/mlb/people.json");
const PLAYER_HITTING: &[u8] = include_bytes!("fixtures/mlb/player-hitting.json");
const PLAYER_PITCHING: &[u8] = include_bytes!("fixtures/mlb/player-pitching.json");
const BULK_HITTING: &[u8] = include_bytes!("fixtures/mlb/bulk-hitting.json");
const BULK_PITCHING: &[u8] = include_bytes!("fixtures/mlb/bulk-pitching.json");
const HITTER_GAME_LOG: &[u8] = include_bytes!("fixtures/mlb/hitter-game-log.json");
const PITCHER_GAME_LOG: &[u8] = include_bytes!("fixtures/mlb/pitcher-game-log.json");

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

struct QualityStartExecutor {
    requests: Mutex<Vec<ValidatedRequest>>,
    active: AtomicUsize,
    maximum: AtomicUsize,
}

impl QualityStartExecutor {
    fn new() -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
        }
    }
}

impl HttpExecutor for QualityStartExecutor {
    fn execute(&self, request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
        let url = reqwest::Url::parse(request.url()).unwrap();
        let path = url.path().to_owned();
        let person_id = path
            .split('/')
            .nth_back(1)
            .and_then(|value| value.parse::<i64>().ok())
            .expect("person ID in statistics path");
        let stats = url
            .query_pairs()
            .find(|(key, _)| key == "stats")
            .map(|(_, value)| value.into_owned())
            .unwrap();
        self.requests.lock().unwrap().push(request);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum.fetch_max(active, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(10));
        self.active.fetch_sub(1, Ordering::SeqCst);
        if person_id == 3 {
            return status(503);
        }
        if person_id == 6 {
            panic!("simulated worker failure");
        }
        if stats == "gameLog" {
            return response(PITCHER_GAME_LOG);
        }
        let quality_starts = person_id % 2;
        Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: format!(
                r#"{{"stats":[{{"splits":[{{"stat":{{"qualityStarts":{quality_starts}}}}}]}}]}}"#
            )
            .into_bytes(),
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
        &[skout::providers::mlb::LineupPlayer {
            person_id: 700001,
            full_name: "Away Hitter".into(),
        }]
    );
    assert_eq!(
        games[0].home_lineup.as_ref().unwrap(),
        &[skout::providers::mlb::LineupPlayer {
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
fn team_directory_preserves_club_names_abbreviations_and_leagues() {
    let executor = Arc::new(QueueExecutor::new(vec![response(TEAM_DIRECTORY)]));
    let client = make_client(executor.clone());
    let teams = client.fetch_team_directory(2026).unwrap();
    assert_eq!(teams.len(), 30);
    assert_eq!(teams[0].abbreviation, "ATH");
    let yankees = teams
        .iter()
        .find(|team| team.abbreviation == "NYY")
        .unwrap();
    assert_eq!(yankees.location_name, "New York");
    assert_eq!(yankees.club_name, "Yankees");
    assert_eq!(yankees.league_id, 103);
    assert!(
        executor.requests.lock().unwrap()[0]
            .url()
            .ends_with("teams?sportId=1&season=2026")
    );
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
    assert_eq!(first.cache_status, MlbCacheStatus::Miss);
    assert!(first.cache_write_issue.is_none());
    let hit = client.fetch_schedule_cached("2026-05-15", &cache).unwrap();
    assert_eq!(hit.cache_status, MlbCacheStatus::Hit);
    assert_eq!(executor.requests.lock().unwrap().len(), 1);
    clock.set(160);
    let expired = client.fetch_schedule_cached("2026-05-15", &cache).unwrap();
    assert_eq!(expired.cache_status, MlbCacheStatus::Expired);
    assert_eq!(executor.requests.lock().unwrap().len(), 2);

    cache
        .put("mlb", "schedule-2026-05-16", b"not json")
        .unwrap();
    let executor = Arc::new(QueueExecutor::new(vec![response(SCHEDULE)]));
    let corrupt = make_client(executor)
        .fetch_schedule_cached("2026-05-16", &cache)
        .unwrap();
    assert_eq!(corrupt.cache_status, MlbCacheStatus::Corrupt);

    cache.put("mlb", "schedule-2026-05-17", SCHEDULE).unwrap();
    let path = cache.entry_path("mlb", "schedule-2026-05-17").unwrap();
    fs::write(path, b"broken frame").unwrap();
    let executor = Arc::new(QueueExecutor::new(vec![response(SCHEDULE)]));
    let corrupt = make_client(executor)
        .fetch_schedule_cached("2026-05-17", &cache)
        .unwrap();
    assert_eq!(corrupt.cache_status, MlbCacheStatus::Corrupt);

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
    assert_eq!(degraded.cache_status, MlbCacheStatus::Miss);
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

#[test]
fn statistics_fixtures_decode_complete_records_and_bulk_identity() {
    let executor = Arc::new(QueueExecutor::new(vec![
        response(PLAYER_HITTING),
        response(PLAYER_PITCHING),
        response(BULK_HITTING),
        response(BULK_PITCHING),
        response(HITTER_GAME_LOG),
        response(PITCHER_GAME_LOG),
    ]));
    let client = make_client(executor);

    assert_eq!(
        client.fetch_hitting_stats(700001, 2026).unwrap(),
        HittingStats {
            games_played: 10,
            plate_appearances: 44,
            at_bats: 40,
            hits: 14,
            home_runs: 3,
            rbi: 11,
            runs: 9,
            stolen_bases: 2,
            average: ".350".into(),
            on_base_percentage: ".409".into(),
            slugging_percentage: ".650".into(),
            on_base_plus_slugging: "1.059".into(),
            strikeouts: 8,
            walks: 4,
            doubles: 2,
            triples: 1,
            caught_stealing: 1,
            hit_by_pitch: 1,
            total_bases: 26,
            sacrifice_flies: 1,
            sacrifice_bunts: 0,
            grounded_into_double_play: 2,
            intentional_walks: 1,
            babip: ".407".into(),
        }
    );
    assert_eq!(
        client.fetch_pitching_stats(600001, 2026).unwrap(),
        PitchingStats {
            games_pitched: 8,
            games_started: 7,
            innings_pitched: "42.2".into(),
            wins: 5,
            losses: 1,
            saves: 0,
            holds: 1,
            strikeouts: 51,
            walks: 9,
            era: "2.11".into(),
            whip: "0.98".into(),
            quality_starts: 6,
            runs: 12,
            hits_allowed: 33,
            earned_runs: 10,
            home_runs_allowed: 4,
            hit_batsmen: 2,
            balks: 1,
            wild_pitches: 3,
            batters_faced: 166,
            games_finished: 1,
            save_opportunities: 1,
            blown_saves: 0,
            complete_games: 1,
            shutouts: 1,
            intentional_walks: 2,
            strikeouts_per_nine: "10.76".into(),
            walks_per_nine: "1.90".into(),
            hits_per_nine: "6.96".into(),
            home_runs_per_nine: "0.84".into(),
            strikeout_walk_ratio: "5.67".into(),
            inherited_runners: 3,
            inherited_runners_scored: 1,
            pickoffs: 2,
            stolen_bases_allowed: 4,
            caught_stealing_allowed: 1,
            number_of_pitches: 650,
            pitches_per_inning: "15.23".into(),
        }
    );

    let hitters = client.fetch_bulk_hitting_stats(2026, "S").unwrap();
    assert_eq!(hitters[0].player.person_id, 700001);
    assert_eq!(hitters[0].player.full_name, "Bulk Hitter");
    assert_eq!(hitters[0].team.team_id, 147);
    assert_eq!(hitters[0].position.position_type, "Fielder");
    assert_eq!(hitters[0].stat.on_base_plus_slugging, "1.205");
    let pitchers = client.fetch_bulk_pitching_stats(2026, "R").unwrap();
    assert_eq!(pitchers[0].player.person_id, 600001);
    assert_eq!(pitchers[0].team.team_id, 110);
    assert_eq!(pitchers[0].position.position_type, "Pitcher");
    assert_eq!(pitchers[0].stat.innings_pitched, "12.1");

    let hitter_log = client.fetch_hitter_game_log(700001, 2026).unwrap();
    assert_eq!(hitter_log[0].date, "2026-04-01");
    assert_eq!(hitter_log[0].game_id, 800010);
    assert!(hitter_log[0].is_home);
    assert_eq!(hitter_log[0].opponent_abbreviation, "BOS");
    assert_eq!(hitter_log[0].stat.on_base_plus_slugging, "2.417");
    let pitcher_log = client.fetch_pitcher_game_log(600001, 2026).unwrap();
    assert_eq!(pitcher_log.len(), 10);
    assert_eq!(pitcher_log[0].game_id, 800011);
    assert!(!pitcher_log[0].is_home);
    assert_eq!(pitcher_log[0].opponent_abbreviation, "TOR");
    assert_eq!(pitcher_log[0].stat.innings_pitched, "6.0");
}

#[test]
fn statistics_requests_use_exact_paths_queries_and_limits() {
    let executor = Arc::new(QueueExecutor::new(vec![
        response(PLAYER_HITTING),
        response(PLAYER_PITCHING),
        response(BULK_HITTING),
        response(BULK_PITCHING),
        response(BULK_HITTING),
        response(BULK_PITCHING),
        response(HITTER_GAME_LOG),
        response(PITCHER_GAME_LOG),
    ]));
    let client = make_client(executor.clone());
    client.fetch_hitting_stats(7, 2026).unwrap();
    client.fetch_pitching_stats(8, 2026).unwrap();
    client.fetch_bulk_hitting_stats(2026, "S").unwrap();
    client.fetch_bulk_pitching_stats(2026, "R").unwrap();
    client
        .fetch_hitting_stats_by_date_range(2026, "2026-04-01", "2026-04-30")
        .unwrap();
    client
        .fetch_pitching_stats_by_date_range(2026, "2026-04-01", "2026-04-30")
        .unwrap();
    client.fetch_hitter_game_log(7, 2026).unwrap();
    client.fetch_pitcher_game_log(8, 2026).unwrap();

    let requests = executor.requests.lock().unwrap();
    let urls = requests
        .iter()
        .map(|request| request.url())
        .collect::<Vec<_>>();
    assert_eq!(
        urls[0],
        "http://127.0.0.1:12345/api/v1/people/7/stats?stats=season&season=2026&group=hitting"
    );
    assert_eq!(
        urls[1],
        "http://127.0.0.1:12345/api/v1/people/8/stats?stats=season&season=2026&group=pitching"
    );
    assert!(urls[2].ends_with(
        "/stats?stats=season&group=hitting&gameType=S&season=2026&playerPool=All&limit=2000"
    ));
    assert!(urls[3].ends_with(
        "/stats?stats=season&group=pitching&gameType=R&season=2026&playerPool=All&limit=2000"
    ));
    assert!(urls[4].ends_with("/stats?stats=byDateRange&group=hitting&gameType=R&season=2026&playerPool=All&limit=2000&startDate=2026-04-01&endDate=2026-04-30"));
    assert!(urls[5].ends_with("/stats?stats=byDateRange&group=pitching&gameType=R&season=2026&playerPool=All&limit=2000&startDate=2026-04-01&endDate=2026-04-30"));
    assert!(urls[6].ends_with("/people/7/stats?stats=gameLog&season=2026&group=hitting"));
    assert!(urls[7].ends_with("/people/8/stats?stats=gameLog&season=2026&group=pitching"));
    for request in requests.iter() {
        assert_eq!(request.timeout(), Duration::from_secs(10));
        assert_eq!(request.body_limit(), 8 * 1024 * 1024);
    }
}

#[test]
fn statistics_empty_envelopes_and_validation_are_explicit() {
    for body in [
        br#"{}"#.as_slice(),
        br#"{"stats":[]}"#.as_slice(),
        br#"{"stats":[{"splits":[]}]}"#.as_slice(),
    ] {
        let executor = Arc::new(QueueExecutor::new(vec![response(body)]));
        let error = make_client(executor)
            .fetch_hitting_stats(1, 2026)
            .unwrap_err();
        assert_eq!(error.operation_name(), "fetch MLB player hitting stats");
        assert!(error.to_string().contains("retry"));
    }
    for body in [
        br#"{}"#.as_slice(),
        br#"{"stats":[]}"#.as_slice(),
        br#"{"stats":[{"splits":[]}]}"#.as_slice(),
    ] {
        let executor = Arc::new(QueueExecutor::new(vec![response(body)]));
        assert!(
            make_client(executor)
                .fetch_bulk_hitting_stats(2026, "R")
                .unwrap()
                .is_empty()
        );
    }
    let executor = Arc::new(QueueExecutor::new(Vec::new()));
    let client = make_client(executor.clone());
    assert!(client.fetch_hitting_stats(0, 2026).is_err());
    assert!(client.fetch_hitting_stats(1, 1875).is_err());
    assert!(client.fetch_bulk_hitting_stats(2026, "P").is_err());
    assert!(
        client
            .fetch_hitting_stats_by_date_range(2026, "2026-02-30", "2026-03-01")
            .is_err()
    );
    assert!(
        client
            .fetch_hitting_stats_by_date_range(2026, "2026-04-02", "2026-04-01")
            .is_err()
    );
    assert!(client.fetch_quality_starts(2026, &[1, 0]).is_err());
    assert!(executor.requests.lock().unwrap().is_empty());
}

#[test]
fn statistics_range_caches_are_typed_separate_and_bounded() {
    let directory = tempdir().unwrap();
    let clock = AdjustableClock::at(100);
    let cache = DiskCache::at_with_clock(directory.path(), Arc::new(clock.clone()));
    let executor = Arc::new(QueueExecutor::new(vec![
        response(BULK_HITTING),
        response(BULK_HITTING),
        response(BULK_HITTING),
    ]));
    let client = make_client(executor.clone());
    let first = client
        .fetch_hitting_stats_by_date_range_cached(2026, "2026-04-01", "2026-04-30", &cache)
        .unwrap();
    assert_eq!(first.cache_status, MlbCacheStatus::Miss);
    let hit = client
        .fetch_hitting_stats_by_date_range_cached(2026, "2026-04-01", "2026-04-30", &cache)
        .unwrap();
    assert_eq!(hit.cache_status, MlbCacheStatus::Hit);
    clock.set(160);
    let expired = client
        .fetch_hitting_stats_by_date_range_cached(2026, "2026-04-01", "2026-04-30", &cache)
        .unwrap();
    assert_eq!(expired.cache_status, MlbCacheStatus::Expired);
    cache
        .put(
            "mlb",
            "hitting-range-2026-2026-05-01-2026-05-31",
            b"bad json",
        )
        .unwrap();
    let corrupt = client
        .fetch_hitting_stats_by_date_range_cached(2026, "2026-05-01", "2026-05-31", &cache)
        .unwrap();
    assert_eq!(corrupt.cache_status, MlbCacheStatus::Corrupt);
    assert_eq!(executor.requests.lock().unwrap().len(), 3);

    let executor = Arc::new(QueueExecutor::new(vec![response(BULK_PITCHING)]));
    let pitching = make_client(executor)
        .fetch_pitching_stats_by_date_range_cached(2026, "2026-04-01", "2026-04-30", &cache)
        .unwrap();
    assert_eq!(pitching.cache_status, MlbCacheStatus::Miss);
    assert_eq!(pitching.splits[0].stat.innings_pitched, "12.1");
}

#[test]
fn quality_start_derivation_rejects_invalid_decimal_outs() {
    let executor = Arc::new(QueueExecutor::new(vec![response(PITCHER_GAME_LOG)]));
    let result = make_client(executor)
        .fetch_quality_starts_by_date_range(2026, "2026-04-01", "2026-05-31", &[600001])
        .unwrap();
    assert_eq!(result.counts.get(&600001), Some(&3));
    assert!(result.issues.is_empty());
}

#[test]
fn statistics_cache_failures_preserve_context_and_live_data() {
    let read_root = tempdir().unwrap();
    fs::write(read_root.path().join("mlb"), b"not a directory").unwrap();
    let read_cache = DiskCache::at(read_root.path());
    let executor = Arc::new(QueueExecutor::new(Vec::new()));
    let error = make_client(executor.clone())
        .fetch_hitting_stats_by_date_range_cached(2026, "2026-04-01", "2026-04-30", &read_cache)
        .unwrap_err();
    assert_eq!(error.operation_name(), "fetch cached MLB hitting stats");
    assert!(executor.requests.lock().unwrap().is_empty());

    let write_root = tempdir().unwrap();
    let write_cache = DiskCache::at(write_root.path());
    let namespace = write_root.path().join("mlb");
    let executor = Arc::new(QueueExecutor::with_hook(
        vec![response(BULK_PITCHING)],
        move || {
            if !namespace.exists() {
                fs::write(&namespace, b"block directory creation").unwrap();
            }
        },
    ));
    let result = make_client(executor)
        .fetch_pitching_stats_by_date_range_cached(2026, "2026-04-01", "2026-04-30", &write_cache)
        .unwrap();
    assert_eq!(result.cache_status, MlbCacheStatus::Miss);
    assert_eq!(result.splits.len(), 1);
    assert!(result.cache_write_issue.is_some());
    assert!(result.cache_write_issue.unwrap().chars().count() <= 256);
}

#[test]
fn statistics_transport_and_json_failures_are_safe() {
    for failure in [
        status(429),
        response(br#"{"stats":["#),
        Err(ExecutorError::ResponseTooLarge { limit: 8 }),
        Err(ExecutorError::Dispatch {
            detail: "secret transport detail".into(),
            source: None,
        }),
    ] {
        let executor = Arc::new(QueueExecutor::new(vec![failure]));
        let error = make_client(executor.clone())
            .fetch_pitching_stats(600001, 2026)
            .unwrap_err();
        assert_eq!(error.operation_name(), "fetch MLB player pitching stats");
        assert!(!error.to_string().contains("secret provider detail"));
        assert_eq!(executor.requests.lock().unwrap().len(), 1);
    }
}

#[test]
fn quality_start_aggregation_is_bounded_ordered_and_partial() {
    let executor = Arc::new(QualityStartExecutor::new());
    let client = MlbClient::new(
        Arc::new(HttpClient::new(executor.clone())),
        MlbEndpoints::new("http://127.0.0.1:12345/api/v1/").unwrap(),
    );
    let season = client
        .fetch_quality_starts(2026, &[1, 2, 3, 4, 5, 6, 7, 1])
        .unwrap();
    assert_eq!(season.counts.get(&1), Some(&3));
    assert_eq!(season.counts.get(&2), Some(&3));
    assert_eq!(season.counts.get(&7), Some(&3));
    assert_eq!(
        season
            .issues
            .iter()
            .map(|issue| issue.person_id)
            .collect::<Vec<_>>(),
        vec![3, 6]
    );
    assert!(
        season
            .issues
            .iter()
            .all(|issue| issue.detail.chars().count() <= 256)
    );
    assert!(season.issues[0].detail.contains("HTTP 503"));
    assert!(
        season.issues[1]
            .detail
            .contains("did not complete normally")
    );
    assert!(executor.maximum.load(Ordering::SeqCst) <= 5);
    assert_eq!(executor.requests.lock().unwrap().len(), 7);

    let before = executor.requests.lock().unwrap().len();
    let empty = client.fetch_quality_starts(2026, &[]).unwrap();
    assert!(empty.counts.is_empty());
    assert!(empty.issues.is_empty());
    assert_eq!(executor.requests.lock().unwrap().len(), before);
}

#[test]
fn date_range_quality_start_partial_results_omit_zero_counts() {
    let executor = Arc::new(QualityStartExecutor::new());
    let client = MlbClient::new(
        Arc::new(HttpClient::new(executor.clone())),
        MlbEndpoints::new("http://127.0.0.1:12345/api/v1/").unwrap(),
    );
    let result = client
        .fetch_quality_starts_by_date_range(
            2026,
            "2026-04-01",
            "2026-04-01",
            &[1, 2, 3, 4, 5, 6, 7],
        )
        .unwrap();
    assert_eq!(result.counts.len(), 5);
    assert!(result.counts.values().all(|count| *count == 1));
    assert_eq!(
        result
            .issues
            .iter()
            .map(|issue| issue.person_id)
            .collect::<Vec<_>>(),
        vec![3, 6]
    );
    assert!(executor.maximum.load(Ordering::SeqCst) <= 5);
}
