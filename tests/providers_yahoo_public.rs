use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use skout::domain::{Position, ScoringType};
use skout::providers::yahoo_fantasy::YahooFantasySource;
use skout::providers::yahoo_public::{
    YahooPublicClient, YahooPublicError, canonical_public_league_key, league_id_from_key,
};
use skout::transport::{ExecutorError, HttpClient, HttpExecutor, HttpResponse, ValidatedRequest};

const REDZONE_VALID: &[u8] = include_bytes!("fixtures/yahoo-public/redzone_valid.json");
const REDZONE_MALFORMED: &[u8] = include_bytes!("fixtures/yahoo-public/redzone_malformed.json");
const REDZONE_NO_TEAMS: &[u8] = include_bytes!("fixtures/yahoo-public/redzone_no_teams.json");
const PUBLIC_RANKS: &[u8] = br#"{"fantasy_content":{"league":{"players":[{"player":{"player_id":"10395","player_ranks":[{"player_rank":{"rank_type":"S","rank_value":"216","rank_season":"2026"}},{"player_rank":{"rank_type":"S","rank_position":"C","rank_value":"12","rank_season":"2026"}}]}}]}}}"#;
const PUBLIC_STANDINGS: &[u8] = br#"{"fantasy_content":{"league":[{"league_key":"mlb.l.1"},{"standings":[{"teams":{"0":{"team":[[{"team_key":"mlb.l.1.t.1"},{"team_id":"1"},{"name":"Operators"},{"waiver_priority":1},{"faab_balance":"65"},{"number_of_moves":29}],{"team_standings":{"rank":1}}]},"1":{"team":[[{"team_key":"mlb.l.1.t.2"},{"team_id":"2"},{"name":"Opponents"},{"waiver_priority":2},{"faab_balance":"33"},{"number_of_moves":56}],{"team_standings":{"rank":2}}]},"count":2}}]}]}}"#;
const PUBLIC_SCOREBOARD: &[u8] = br#"{"fantasy_content":{"league":[{"league_key":"mlb.l.1"},{"scoreboard":{"0":{"matchups":{"0":{"matchup":{"week":"7","week_start":"2026-05-11","week_end":"2026-05-17","status":"midevent","0":{"teams":{"0":{"team":[[{"team_key":"mlb.l.1.t.1"},{"team_id":"1"},{"name":"Operators"}],{"team_stats":{"stats":[{"stat":{"stat_id":"7","value":"12"}}]}}]},"1":{"team":[[{"team_key":"mlb.l.1.t.2"},{"team_id":"2"},{"name":"Opponents"}],{"team_stats":{"stats":[{"stat":{"stat_id":"7","value":"9"}}]}}]}}}}},"1":{"matchup":{"week":"7","week_start":"2026-05-11","week_end":"2026-05-17","status":"midevent","0":{"teams":{"0":{"team":[[{"team_key":"mlb.l.1.t.5"},{"team_id":"5"},{"name":"Another Team"}],{"team_stats":{"stats":[{"stat":{"stat_id":"7","value":"8"}}]}}]},"1":{"team":[[{"team_key":"mlb.l.1.t.6"},{"team_id":"6"},{"name":"Toros"}],{"team_stats":{"stats":[{"stat":{"stat_id":"7","value":"11"}}]}}]}}}}}}},"week":"7"}}]}}"#;

const LEAGUE_SETTINGS: &[u8] = include_bytes!("fixtures/yahoo/league-settings.json");
const LEAGUE_STANDINGS: &[u8] = include_bytes!("fixtures/yahoo/standings.json");
const ROSTER_TEAM_1: &[u8] = include_bytes!("fixtures/yahoo/roster-team-1.json");
const ROSTER_TEAM_2: &[u8] = include_bytes!("fixtures/yahoo/roster-team-2.json");
const FREE_AGENTS: &[u8] = include_bytes!("fixtures/yahoo/free-agents.json");
const MATCHUP: &[u8] = include_bytes!("fixtures/yahoo/matchup.json");
const WEEKLY_STATS: &[u8] = include_bytes!("fixtures/yahoo/weekly-stats.json");
const NO_FREE_AGENTS: &[u8] = br#"{"fantasy_content":{"league":[{}, {"players":{"count":0}}]}}"#;

// Fixture provenance: hand-built from the confirmed real response shape of
// `pub-api.fantasysports.yahoo.com/fantasy/v3/redzone/mlb`, trimmed to two
// teams and a handful of players. Manager nicknames were already redacted by
// Yahoo (`--hidden--`) in the real response; no other PII is present.

type RecordedRequest = (
    String,
    Vec<skout::transport::HttpHeader>,
    std::time::Duration,
    usize,
);

struct FakeExecutor {
    responses: Mutex<VecDeque<Result<HttpResponse, ExecutorError>>>,
    requests: Mutex<Vec<RecordedRequest>>,
}

impl FakeExecutor {
    fn new(responses: Vec<Result<HttpResponse, ExecutorError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl HttpExecutor for FakeExecutor {
    fn execute(&self, request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
        self.requests.lock().unwrap().push((
            request.url().to_owned(),
            request.headers(),
            request.timeout(),
            request.body_limit(),
        ));
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("fake response available")
    }
}

fn response(status: u16, body: &[u8]) -> Result<HttpResponse, ExecutorError> {
    Ok(HttpResponse {
        status,
        headers: Vec::new(),
        body: body.to_vec(),
    })
}

fn client(executor: Arc<FakeExecutor>) -> YahooPublicClient {
    YahooPublicClient::new(HttpClient::new(executor))
}

#[test]
fn valid_fixture_parses_league_teams_rosters_and_matchup_pairing() {
    let executor = Arc::new(FakeExecutor::new(vec![response(200, REDZONE_VALID)]));
    let feed = client(executor.clone())
        .fetch_redzone("170874", "469.l.170874")
        .unwrap();

    assert_eq!(feed.league.league_key, "469.l.170874");
    assert_eq!(feed.league.name, "Yahoo Prize H2H-Cat 170874");
    assert_eq!(feed.league.season, 2026);
    assert_eq!(feed.league.num_teams, 2);
    assert_eq!(feed.league.scoring_type, ScoringType::HeadToHead);

    assert_eq!(feed.teams.len(), 2);
    let yankees = feed
        .teams
        .iter()
        .find(|team| team.name == "New York Yankees")
        .unwrap();
    assert_eq!(yankees.wins, 85);
    assert_eq!(yankees.losses, 92);
    assert_eq!(yankees.ties, 13);
    assert_eq!(yankees.rank, 1);
    assert_eq!(yankees.manager_name, "--hidden--");
    assert_eq!(yankees.team_key, "469.l.170874.t.1");

    // The invalid/empty roster slot and Yahoo's recently-dropped `--` row
    // are skipped. Team 1 has 3 real rostered players.
    let yankees_slots: Vec<_> = feed
        .slots
        .iter()
        .filter(|slot| slot.team_key == "469.l.170874.t.1")
        .collect();
    assert_eq!(yankees_slots.len(), 3);
    assert!(
        feed.players
            .iter()
            .all(|player| player.yahoo_player_id != 64813)
    );

    let kelly = feed
        .players
        .iter()
        .find(|player| player.yahoo_player_id == 10395)
        .unwrap();
    assert_eq!(kelly.name, "Carson Kelly");
    assert_eq!(kelly.mlb_team, "CHC");
    assert!(kelly.eligible_positions.contains(&Position::Catcher));
    assert_eq!(kelly.percent_owned, None);
    assert_eq!(kelly.yahoo_rank, None);

    let injured = feed
        .players
        .iter()
        .find(|player| player.yahoo_player_id == 60129)
        .unwrap();
    assert_eq!(injured.injury_status, "IL");

    // No cookies, no auth header — a normal Accept header only.
    let requests = executor.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].2, std::time::Duration::from_secs(10));
    assert!(requests[0].0.contains("league_id=170874"));
    assert!(requests[0].0.contains("format=json"));
    for header in &requests[0].1 {
        let name = header.name.to_ascii_lowercase();
        assert_ne!(name, "cookie");
        assert_ne!(name, "authorization");
    }
}

#[test]
fn public_rank_supplement_batches_roster_ids_and_ignores_position_rank() {
    let executor = Arc::new(FakeExecutor::new(vec![
        response(200, REDZONE_VALID),
        response(200, PUBLIC_RANKS),
    ]));
    let client = client(executor.clone());
    let mut feed = client.fetch_redzone("170874", "469.l.170874").unwrap();

    client.enrich_player_ranks(&mut feed.players).unwrap();

    assert_eq!(
        feed.players
            .iter()
            .find(|player| player.yahoo_player_id == 10395)
            .and_then(|player| player.yahoo_rank),
        Some(216)
    );
    let requests = executor.requests();
    assert!(
        requests
            .iter()
            .all(|request| request.2 == std::time::Duration::from_secs(10))
    );
    assert!(
        requests[1]
            .0
            .contains("league/mlb.l.public/players;player_ids=")
    );
    assert!(
        requests[1]
            .0
            .contains(";out=ranks;ranks=season?format=json_f")
    );
}

#[test]
fn public_standings_supplement_populates_budget_waiver_and_moves_without_auth() {
    let executor = Arc::new(FakeExecutor::new(vec![
        response(200, REDZONE_VALID),
        response(200, PUBLIC_STANDINGS),
    ]));
    let client = client(executor.clone());
    let mut feed = client.fetch_redzone("170874", "mlb.l.1").unwrap();

    client
        .enrich_team_transactions("mlb.l.1", &mut feed.teams)
        .unwrap();

    let yankees = feed
        .teams
        .iter()
        .find(|team| team.team_key == "mlb.l.1.t.1")
        .unwrap();
    assert_eq!(yankees.faab_balance, 65);
    assert_eq!(yankees.waiver_priority, 1);
    assert_eq!(yankees.moves, 29);
    let requests = executor.requests();
    assert!(
        requests
            .iter()
            .all(|request| request.2 == std::time::Duration::from_secs(10))
    );
    assert_eq!(
        requests[1].0,
        "https://pub-api-ro.fantasysports.yahoo.com/fantasy/v2/league/mlb.l.1/standings?format=json"
    );
    assert!(
        requests[1]
            .1
            .iter()
            .all(|header| !header.name.eq_ignore_ascii_case("authorization"))
    );
}

#[test]
fn matchup_aggregation_sums_counting_stats_computes_rate_stats_and_scores_categories() {
    let executor = Arc::new(FakeExecutor::new(vec![response(200, REDZONE_VALID)]));
    let feed = client(executor)
        .fetch_redzone("170874", "469.l.170874")
        .unwrap();

    assert_eq!(feed.matchups.len(), 1);
    let matchup = &feed.matchups[0];
    assert_eq!(matchup.week, 20);
    assert_eq!(matchup.week_start, "2026-08-10");
    assert_eq!(matchup.week_end, "2026-08-16");

    let yankees = matchup
        .teams
        .iter()
        .find(|team| team.team_key == "469.l.170874.t.1")
        .unwrap();
    let vladdy = matchup
        .teams
        .iter()
        .find(|team| team.team_key == "469.l.170874.t.2")
        .unwrap();

    // Counting stats: only the active roster (catcher + pitcher; the IL
    // player's 99s are excluded) sums R=5, HR=2, RBI=8, SB=1, W=1, SV=0, K=10.
    assert_eq!(yankees.stats.get("7").unwrap(), "5");
    assert_eq!(yankees.stats.get("12").unwrap(), "2");
    assert_eq!(yankees.stats.get("13").unwrap(), "8");
    assert_eq!(yankees.stats.get("16").unwrap(), "1");
    assert_eq!(yankees.stats.get("28").unwrap(), "1");
    assert_eq!(yankees.stats.get("32").unwrap(), "0");
    assert_eq!(yankees.stats.get("42").unwrap(), "10");

    // Rate stats computed from summed counting stats, not summed/averaged
    // directly: AVG = 10H / 20AB = .500; ERA = 9*2ER / 6IP = 3.00;
    // WHIP = (3BB + 5H) / 6IP = 1.33.
    assert_eq!(yankees.stats.get("3").unwrap(), "0.500");
    assert_eq!(yankees.stats.get("H/AB").unwrap(), "10/20");
    assert_eq!(yankees.stats.get("26").unwrap(), "3.00");
    assert_eq!(yankees.stats.get("27").unwrap(), "1.33");
    assert_eq!(yankees.stats.get("50").unwrap(), "6.0");

    // Building-block ids (AB, batting H) are not their own scoring category.
    assert!(!yankees.stats.contains_key("6"));
    assert!(!yankees.stats.contains_key("8"));
    // Confirmed non-scoring display-only stat is never included.
    assert!(!yankees.stats.contains_key("60"));

    assert_eq!(vladdy.stats.get("3").unwrap(), "0.300");
    assert_eq!(vladdy.stats.get("26").unwrap(), "4.50");
    assert_eq!(vladdy.stats.get("27").unwrap(), "1.00");

    // Category-by-category: Yankees win R, HR, RBI, SB, AVG, W, K, ERA (8);
    // Vladdy wins SV (higher) and WHIP (1.00 < 1.33, lower is better) (2).
    assert_eq!(yankees.wins, 8);
    assert_eq!(yankees.losses, 2);
    assert_eq!(yankees.ties, 0);
    assert_eq!(vladdy.wins, 2);
    assert_eq!(vladdy.losses, 8);
    assert_eq!(vladdy.ties, 0);

    // Per-player boxscore for both rosters.
    let yankees_roster = feed
        .roster_week_stats
        .get("469.l.170874.t.1")
        .expect("yankees roster");
    assert_eq!(yankees_roster.week, 20);
    // The invalid slot is skipped; the IL player still appears (rosters show
    // injured players, they just don't count toward the team's category
    // totals above).
    assert_eq!(yankees_roster.players.len(), 3);
    let pitcher = yankees_roster
        .players
        .iter()
        .find(|player| player.yahoo_player_id == 70001)
        .unwrap();
    assert_eq!(pitcher.innings_pitched, "6.0");
    assert_eq!(pitcher.earned_run_average, "3.00");
    assert_eq!(pitcher.whip, "1.33");
    assert_eq!(pitcher.strikeouts, 10);
    let batter = yankees_roster
        .players
        .iter()
        .find(|player| player.yahoo_player_id == 10395)
        .unwrap();
    assert_eq!(batter.hab, "10-20");
    assert_eq!(batter.batting_average, "0.500");
    assert_eq!(batter.runs, 5);
}

#[test]
fn public_scoreboard_uses_authoritative_team_totals_without_auth() {
    let executor = Arc::new(FakeExecutor::new(vec![response(200, PUBLIC_SCOREBOARD)]));
    let matchups = client(executor.clone())
        .fetch_scoreboard("mlb.l.1", 7)
        .unwrap();

    assert_eq!(matchups[0].teams[0].stats.get("7").unwrap(), "12");
    assert_eq!(matchups[0].teams[1].stats.get("7").unwrap(), "9");
    assert_eq!(
        (
            matchups[0].teams[0].wins,
            matchups[0].teams[0].ties,
            matchups[0].teams[0].losses,
        ),
        (1, 9, 0)
    );
    assert_eq!(matchups.len(), 2);
    assert!(
        matchups[1]
            .teams
            .iter()
            .any(|team| team.team_key == "mlb.l.1.t.6"
                && team.stats.get("7").map(String::as_str) == Some("11"))
    );
    let requests = executor.requests();
    assert_eq!(requests[0].2, std::time::Duration::from_secs(10));
    assert_eq!(
        requests[0].0,
        "https://pub-api-ro.fantasysports.yahoo.com/fantasy/v2/league/mlb.l.1/scoreboard;week=7?format=json"
    );
    assert!(
        requests[0]
            .1
            .iter()
            .all(|header| !header.name.eq_ignore_ascii_case("authorization"))
    );
}

#[test]
fn malformed_response_fails_with_context() {
    let executor = Arc::new(FakeExecutor::new(vec![response(200, REDZONE_MALFORMED)]));
    let error = client(executor)
        .fetch_redzone("170874", "469.l.170874")
        .unwrap_err();
    assert!(matches!(error, YahooPublicError::Malformed(_)));
}

#[test]
fn a_league_with_no_teams_is_reported_incomplete() {
    let executor = Arc::new(FakeExecutor::new(vec![response(200, REDZONE_NO_TEAMS)]));
    let error = client(executor)
        .fetch_redzone("170874", "469.l.170874")
        .unwrap_err();
    assert!(matches!(error, YahooPublicError::Incomplete(_)));
}

#[test]
fn missing_league_entry_is_reported_incomplete() {
    let executor = Arc::new(FakeExecutor::new(vec![response(200, REDZONE_VALID)]));
    let error = client(executor)
        .fetch_redzone("999999", "public.999999")
        .unwrap_err();
    assert!(matches!(error, YahooPublicError::Incomplete(_)));
}

#[test]
fn non_200_status_is_reported_as_blocked_not_escalated() {
    for status in [401, 403, 429, 500, 503] {
        let executor = Arc::new(FakeExecutor::new(vec![response(status, b"")]));
        let error = client(executor)
            .fetch_redzone("170874", "469.l.170874")
            .unwrap_err();
        assert!(matches!(error, YahooPublicError::Blocked { status: s } if s == status));
    }
}

#[test]
fn league_id_from_key_round_trips_through_the_client_call() {
    assert_eq!(league_id_from_key("469.l.170874").unwrap(), "170874");
    assert_eq!(
        canonical_public_league_key("170874").unwrap(),
        "mlb.l.170874"
    );
    assert_eq!(
        canonical_public_league_key("public.170874").unwrap(),
        "mlb.l.170874"
    );
    assert_eq!(
        canonical_public_league_key("469.l.170874").unwrap(),
        "469.l.170874"
    );
    assert!(league_id_from_key("garbage").is_err());
}

#[test]
fn fantasy_source_uses_exact_public_paths_without_credentials() {
    let executor = Arc::new(FakeExecutor::new(vec![
        response(200, LEAGUE_SETTINGS),
        response(200, LEAGUE_STANDINGS),
        response(200, ROSTER_TEAM_1),
        response(200, ROSTER_TEAM_2),
        response(200, FREE_AGENTS),
        response(200, NO_FREE_AGENTS),
        response(200, MATCHUP),
        response(200, WEEKLY_STATS),
        response(200, WEEKLY_STATS),
    ]));
    let client = client(executor.clone());

    assert_eq!(
        client.league_settings("mlb.l.1").unwrap().league.league_key,
        "mlb.l.1"
    );
    let standings = client.standings("mlb.l.1").unwrap();
    assert_eq!(standings.len(), 2);
    let team_keys: Vec<String> = standings.iter().map(|team| team.team_key.clone()).collect();
    assert_eq!(
        client
            .league_rosters("mlb.l.1", &team_keys)
            .unwrap()
            .slots
            .len(),
        2
    );
    assert!(!client.free_agents("mlb.l.1").unwrap().is_empty());
    assert_eq!(client.scoreboard("mlb.l.1", Some(7)).unwrap().len(), 1);
    assert_eq!(
        client
            .roster_week_stats("mlb.l.1.t.1", 7)
            .unwrap()
            .players
            .len(),
        1
    );
    assert_eq!(
        client
            .roster_day_stats("mlb.l.1.t.1", 7, "2026-05-11")
            .unwrap()
            .players
            .len(),
        1
    );

    let requests = executor.requests();
    let urls = requests
        .into_iter()
        .map(|(url, headers, _, _)| {
            assert!(headers.iter().all(|header| {
                !header.name.eq_ignore_ascii_case("authorization")
                    && !header.name.eq_ignore_ascii_case("cookie")
            }));
            url
        })
        .collect::<Vec<_>>();
    assert_eq!(
        urls,
        vec![
            "https://pub-api-ro.fantasysports.yahoo.com/fantasy/v2/league/mlb.l.1/settings?format=json",
            "https://pub-api-ro.fantasysports.yahoo.com/fantasy/v2/league/mlb.l.1/standings?format=json",
            "https://pub-api-ro.fantasysports.yahoo.com/fantasy/v2/team/mlb.l.1.t.1/roster/players;out=ranks,percent_owned,percent_started?format=json",
            "https://pub-api-ro.fantasysports.yahoo.com/fantasy/v2/team/mlb.l.1.t.2/roster/players;out=ranks,percent_owned,percent_started?format=json",
            "https://pub-api-ro.fantasysports.yahoo.com/fantasy/v2/league/mlb.l.1/players;status=A;start=0;count=25;out=ranks,percent_owned,percent_started?format=json",
            "https://pub-api-ro.fantasysports.yahoo.com/fantasy/v2/league/mlb.l.1/players;status=A;start=25;count=25;out=ranks,percent_owned,percent_started?format=json",
            "https://pub-api-ro.fantasysports.yahoo.com/fantasy/v2/league/mlb.l.1/scoreboard;week=7?format=json",
            "https://pub-api-ro.fantasysports.yahoo.com/fantasy/v2/team/mlb.l.1.t.1/roster;week=7/players/stats;type=week;week=7?format=json",
            "https://pub-api-ro.fantasysports.yahoo.com/fantasy/v2/team/mlb.l.1.t.1/roster;date=2026-05-11/players/stats;type=date;date=2026-05-11?format=json",
        ]
    );
    let requests = executor.requests();
    assert!(
        requests
            .iter()
            .all(|request| request.2 == std::time::Duration::from_secs(10))
    );
    assert!(requests.iter().all(|request| request.3 == 8 * 1024 * 1024));
}
