use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use b9::domain::{Position, ScoringType};
use b9::providers::yahoo_public::{YahooPublicClient, YahooPublicError, league_id_from_key};
use b9::transport::{ExecutorError, HttpClient, HttpExecutor, HttpResponse, ValidatedRequest};

const REDZONE_VALID: &[u8] = include_bytes!("fixtures/yahoo-public/redzone_valid.json");
const REDZONE_MALFORMED: &[u8] = include_bytes!("fixtures/yahoo-public/redzone_malformed.json");
const REDZONE_NO_TEAMS: &[u8] = include_bytes!("fixtures/yahoo-public/redzone_no_teams.json");

// Fixture provenance: hand-built from the confirmed real response shape of
// `pub-api.fantasysports.yahoo.com/fantasy/v3/redzone/mlb`, trimmed to two
// teams and a handful of players. Manager nicknames were already redacted by
// Yahoo (`--hidden--`) in the real response; no other PII is present.

struct FakeExecutor {
    responses: Mutex<VecDeque<Result<HttpResponse, ExecutorError>>>,
    requests: Mutex<Vec<(String, Vec<b9::transport::HttpHeader>)>>,
}

impl FakeExecutor {
    fn new(responses: Vec<Result<HttpResponse, ExecutorError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<(String, Vec<b9::transport::HttpHeader>)> {
        self.requests.lock().unwrap().clone()
    }
}

impl HttpExecutor for FakeExecutor {
    fn execute(&self, request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
        self.requests
            .lock()
            .unwrap()
            .push((request.url().to_owned(), request.headers()));
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

    // The invalid/empty roster slot (id: null, positionType: false) is
    // skipped, not fabricated into a player. Team 1 has 3 real rostered
    // players (catcher, an IL batter, a pitcher) plus the one invalid slot.
    let yankees_slots: Vec<_> = feed
        .slots
        .iter()
        .filter(|slot| slot.team_key == "469.l.170874.t.1")
        .collect();
    assert_eq!(yankees_slots.len(), 3);

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
    assert!(requests[0].0.contains("league_id=170874"));
    assert!(requests[0].0.contains("format=json"));
    for header in &requests[0].1 {
        let name = header.name.to_ascii_lowercase();
        assert_ne!(name, "cookie");
        assert_ne!(name, "authorization");
    }
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
    assert!(league_id_from_key("garbage").is_err());
}
