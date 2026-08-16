use std::collections::HashMap;

use b9::advisory::{AdvisoryAction, AdvisoryResponse};
use b9::domain::{Matchup, MatchupTeam, PlayerWeekStats, Position, RosterWeekStats};
use std::time::{Duration, SystemTime};

use b9::matchup::{
    LocalMatchupView, MatchupOptions, MatchupView, cached_or_fetch_at, render_advisory_response,
    render_local_matchup, render_matchup,
};
use b9::store::Store;
use b9::terminal::HelpColorMode;
use tempfile::tempdir;

fn team(key: &str, name: &str, mine: bool, wins: i32, losses: i32) -> MatchupTeam {
    MatchupTeam {
        team_key: key.into(),
        team_id: if mine { 1 } else { 2 },
        name: name.into(),
        is_current_login: mine,
        stats: HashMap::new(),
        wins,
        losses,
        ties: 1,
        completed_games: 4,
        live_games: 1,
        remaining_games: 5,
    }
}

fn player(id: i64, name: &str, role: &str, position: Position) -> PlayerWeekStats {
    PlayerWeekStats {
        yahoo_player_id: id,
        name: name.into(),
        team: String::new(),
        position_type: role.into(),
        slot_position: position,
        eligible_positions: vec![],
        injury_status: String::new(),
        hab: String::new(),
        runs: 0,
        home_runs: 0,
        runs_batted_in: 0,
        stolen_bases: 0,
        batting_average: String::new(),
        innings_pitched: String::new(),
        wins: 0,
        saves: 0,
        strikeouts: 0,
        earned_run_average: String::new(),
        whip: String::new(),
    }
}

#[test]
fn baseline_matchup_is_deterministic_and_marks_stale_data() {
    let matchup = Matchup {
        week: 7,
        week_start: String::new(),
        week_end: String::new(),
        status: "midevent".into(),
        teams: [
            team("one", "Operators", true, 5, 4),
            team("two", "Opponents", false, 4, 5),
        ],
    };
    let mine = RosterWeekStats {
        team_key: "one".into(),
        team_name: "Operators".into(),
        week: 7,
        players: vec![player(1, "Ada Hitter", "B", Position::Outfield)],
    };
    let opponent = RosterWeekStats {
        team_key: "two".into(),
        team_name: "Opponents".into(),
        week: 7,
        players: vec![player(2, "Grace Pitcher", "P", Position::StartingPitcher)],
    };
    let fresh = render_matchup(
        &MatchupView {
            matchup: matchup.clone(),
            mine: mine.clone(),
            opponent: opponent.clone(),
            stale: false,
            odds: vec![],
        },
        HelpColorMode::Plain,
    );
    assert_eq!(fresh, include_str!("fixtures/matchup/current.txt"));
    let stale = render_matchup(
        &MatchupView {
            matchup,
            mine,
            opponent,
            stale: true,
            odds: vec![],
        },
        HelpColorMode::Plain,
    );
    assert_eq!(stale, include_str!("fixtures/matchup/weekly.txt"));
    let colored = render_matchup(
        &MatchupView {
            matchup: Matchup {
                week: 7,
                week_start: String::new(),
                week_end: String::new(),
                status: String::new(),
                teams: [
                    team("one", "Operators", true, 5, 4),
                    team("two", "Opponents", false, 4, 5),
                ],
            },
            mine: RosterWeekStats {
                team_key: "one".into(),
                team_name: "Operators".into(),
                week: 7,
                players: vec![],
            },
            opponent: RosterWeekStats {
                team_key: "two".into(),
                team_name: "Opponents".into(),
                week: 7,
                players: vec![],
            },
            stale: false,
            odds: vec![],
        },
        HelpColorMode::Color,
    );
    assert!(colored.contains("\u{1b}[1;38;5;231mMATCHUP WEEK 7\u{1b}[0m"));
    assert!(colored.contains("\u{1b}[38;5;33mHITTER"));
    assert!(colored.contains("\u{1b}[38;5;245m  H/AB\u{1b}[0m"));
    assert!(colored.contains("\u{1b}[38;5;245mSLOT\u{1b}[0m"));
}

#[test]
fn matchup_surfaces_strip_team_name_emoji_from_cached_views() {
    let matchup = Matchup {
        week: 7,
        week_start: String::new(),
        week_end: String::new(),
        status: "midevent".into(),
        teams: [
            team("one", "💎 Operators", true, 5, 4),
            team("two", "⚾ Opponents", false, 4, 5),
        ],
    };
    let roster = |key: &str, name: &str| RosterWeekStats {
        team_key: key.into(),
        team_name: name.into(),
        week: 7,
        players: vec![],
    };
    let output = render_matchup(
        &MatchupView {
            matchup,
            mine: roster("one", "💎 Operators"),
            opponent: roster("two", "⚾ Opponents"),
            stale: true,
            odds: vec![],
        },
        HelpColorMode::Plain,
    );
    assert!(output.contains("Operators  5–4–1    Opponents  4–5–1"));
    assert!(!output.contains('💎'));
    assert!(!output.contains('⚾'));

    let local = render_local_matchup(
        &LocalMatchupView {
            team_name: "💎 Operators".into(),
            players: vec![],
        },
        HelpColorMode::Plain,
    );
    assert!(local.starts_with("Operators\n"));
}

#[test]
fn matchup_snapshot_honors_sixty_seconds_and_falls_back_stale() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
    let now = SystemTime::now();
    let (first, stale) = cached_or_fetch_at(&mut store, "match_test", "week", now, || {
        Ok::<_, &str>(vec![1, 2])
    })
    .unwrap();
    assert_eq!(first, vec![1, 2]);
    assert!(!stale);
    let (cached, stale) = cached_or_fetch_at(
        &mut store,
        "match_test",
        "week",
        now + Duration::from_secs(59),
        || Err::<Vec<i32>, _>("must not fetch"),
    )
    .unwrap();
    assert_eq!(cached, vec![1, 2]);
    assert!(!stale);
    let (fallback, stale) = cached_or_fetch_at(
        &mut store,
        "match_test",
        "week",
        now + Duration::from_secs(61),
        || Err::<Vec<i32>, _>("offline"),
    )
    .unwrap();
    assert_eq!(fallback, vec![1, 2]);
    assert!(stale);
}

#[test]
fn matchup_period_options_reject_ambiguous_or_invalid_selectors() {
    assert!(
        MatchupOptions {
            day: Some("2026-04-01".into()),
            ..MatchupOptions::default()
        }
        .validate()
        .is_ok()
    );
    assert!(
        MatchupOptions {
            week: Some(2),
            day: Some("2026-04-01".into()),
            ..MatchupOptions::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        MatchupOptions {
            day: Some("2026-13-40".into()),
            ..MatchupOptions::default()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn local_fallback_omits_opponent_categories_and_advisory() {
    let output = render_local_matchup(
        &LocalMatchupView {
            team_name: "Operators".into(),
            players: vec![player(1, "Ada Hitter", "B", Position::Outfield)],
        },
        HelpColorMode::Plain,
    );
    assert!(output.contains("LOCAL ROSTER"));
    assert!(!output.contains("CATEGORIES"));
    assert!(!output.contains("ADVIS"));
    assert!(!output.contains("|"));
}

#[test]
fn advisory_surface_renders_only_grounded_response_fields() {
    let mut output = String::new();
    render_advisory_response(
        &mut output,
        &AdvisoryResponse {
            confirmations: vec!["Leading HR".into()],
            urgent: vec![AdvisoryAction {
                id: "lineup-0".into(),
                summary: "Start Ada".into(),
            }],
            overnight: Vec::new(),
            risks: vec!["Ben is injured".into()],
        },
        HelpColorMode::Plain,
    );
    assert!(output.contains("ADVICE"));
    assert!(output.contains("Leading HR"));
    assert!(output.contains("Urgent: Start Ada"));
    assert!(output.contains("Risk: Ben is injured"));
}
