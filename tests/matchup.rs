use std::collections::HashMap;

use b9::domain::{
    GameIndicator, Matchup, MatchupTeam, PlayerWeekStats, Position, RosterWeekStats,
    StoredFantasyPlayer,
};
use std::time::{Duration, SystemTime};

use b9::matchup::{
    LocalMatchupView, MatchupOptions, MatchupView, cached_or_fetch_at, render_local_matchup,
    render_matchup,
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

fn stored_hitter() -> StoredFantasyPlayer {
    StoredFantasyPlayer {
        yahoo_player_id: Some(1),
        mlbam_id: Some(2),
        name: "Ada Hitter".into(),
        team: "NYY".into(),
        role: "B".into(),
        positions: "OF".into(),
        is_closer: false,
        status: String::new(),
        injury_note: String::new(),
        birth_date: String::new(),
        game_status: "7:05p   v BOS".into(),
        game_indicator: GameIndicator::None,
        hand: "R".into(),
        rank: Some(12),
        percent_owned: None,
        percentage_started: 0.0,
        expert_consensus_rank: None,
        owner: Some("Operators".into()),
        slot: Some("OF".into()),
        batting: [100.0, 0.35, 20.0, 5.0, 18.0, 3.0, 0.275],
        pitching: [0.0; 7],
        hitting_advanced: [None; 8],
        pitching_advanced: [None; 6],
        fangraphs_batted_ball: [None; 2],
        pqs_counting: [0.0; 6],
        statcast_samples: [0.0; 4],
        pqs_prior_counting: [0.0; 6],
        league_games_played: 0,
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
        week_start: "2026-05-04".into(),
        week_end: "2026-05-10".into(),
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
            teams: vec![],
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
            teams: vec![],
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
            teams: vec![],
            stale: false,
            odds: vec![],
        },
        HelpColorMode::Color,
    );
    assert!(colored.contains("\u{1b}[38;5;33mMATCHUP WEEK:\u{1b}[0m"));
    assert!(colored.contains("\u{1b}[38;5;33mHITTER"));
    assert!(colored.contains("\u{1b}[38;5;245m  H/AB\u{1b}[0m"));
    assert!(colored.contains("\u{1b}[38;5;245mSLOT\u{1b}[0m"));
}

#[test]
fn matchup_name_columns_use_first_initial_and_leave_space_before_status() {
    let matchup = Matchup {
        week: 7,
        week_start: "2026-05-04".into(),
        week_end: "2026-05-10".into(),
        status: "midevent".into(),
        teams: [
            team("one", "Operators", true, 5, 4),
            team("two", "Opponents", false, 4, 5),
        ],
    };
    let mut hitter = player(1, "Fernando Tatis Jr. SD", "B", Position::Outfield);
    hitter.injury_status = "7:10p   v BOS".into();
    let mut pitcher = player(2, "Braxton Ashcraft PIT", "P", Position::StartingPitcher);
    pitcher.injury_status = "6:40p   v DET".into();
    let output = render_matchup(
        &MatchupView {
            matchup,
            mine: RosterWeekStats {
                team_key: "one".into(),
                team_name: "Operators".into(),
                week: 7,
                players: vec![hitter, pitcher],
            },
            opponent: RosterWeekStats {
                team_key: "two".into(),
                team_name: "Opponents".into(),
                week: 7,
                players: vec![],
            },
            teams: vec![],
            stale: false,
            odds: vec![],
        },
        HelpColorMode::Plain,
    );

    assert!(output.contains("F Tatis Jr. SD      7:10p   v BOS"));
    assert!(output.contains("B Ashcraft PIT      6:40p   v DET"));
}

#[test]
fn matchup_colors_batting_order_and_probable_starter_markers() {
    let matchup = Matchup {
        week: 7,
        week_start: "2026-05-04".into(),
        week_end: "2026-05-10".into(),
        status: "midevent".into(),
        teams: [
            team("one", "Operators", true, 5, 4),
            team("two", "Opponents", false, 4, 5),
        ],
    };
    let mut hitter = player(1, "Ada Hitter NYY", "B", Position::Outfield);
    hitter.injury_status = "7:10p 3 @ BOS".into();
    let mut excluded = player(2, "Grace Hitter BOS", "B", Position::Outfield);
    excluded.injury_status = "7:10p ● v NYY".into();
    let mut starter = player(3, "Linus Pitcher NYM", "P", Position::StartingPitcher);
    starter.injury_status = "7:10p ● v SD".into();

    let colored = render_matchup(
        &MatchupView {
            matchup,
            mine: RosterWeekStats {
                team_key: "one".into(),
                team_name: "Operators".into(),
                week: 7,
                players: vec![hitter, starter],
            },
            opponent: RosterWeekStats {
                team_key: "two".into(),
                team_name: "Opponents".into(),
                week: 7,
                players: vec![excluded],
            },
            teams: vec![],
            stale: false,
            odds: vec![],
        },
        HelpColorMode::Color,
    );

    assert!(colored.contains("7:10p \u{1b}[38;5;46m3\u{1b}[0m @ BOS"));
    assert!(colored.contains("7:10p \u{1b}[38;5;196m●\u{1b}[0m v NYY"));
    assert!(colored.contains("7:10p \u{1b}[38;5;46m●\u{1b}[0m v SD"));
}

#[test]
fn matchup_renders_named_category_totals_inline_with_winner_colors() {
    let mut mine = team("one", "Operators", true, 5, 4);
    let mut opponent = team("two", "Opponents", false, 4, 5);
    for (name, mine_value, opponent_value) in [
        ("H/AB", "8/20", "7/20"),
        ("R", "6", "4"),
        ("HR", "2", "2"),
        ("RBI", "8", "5"),
        ("SB", "1", "3"),
        ("AVG", ".400", ".350"),
        ("IP", "11.0", "9.0"),
        ("W", "1", "0"),
        ("SV", "1", "1"),
        ("K", "9", "10"),
        ("ERA", "6.55", "3.00"),
        ("WHIP", "1.55", "1.20"),
    ] {
        mine.stats.insert(name.into(), mine_value.into());
        opponent.stats.insert(name.into(), opponent_value.into());
    }
    let roster = |key: &str, name: &str| RosterWeekStats {
        team_key: key.into(),
        team_name: name.into(),
        week: 7,
        players: vec![],
    };
    let view = MatchupView {
        matchup: Matchup {
            week: 7,
            week_start: String::new(),
            week_end: String::new(),
            status: String::new(),
            teams: [mine, opponent],
        },
        mine: roster("one", "Operators"),
        opponent: roster("two", "Opponents"),
        teams: vec![],
        stale: false,
        odds: vec![b9::matchup::MatchupOdds {
            mine: true,
            line: "Cole             v Bello             NYY@BOS  ██████░░░░ 55%".into(),
        }],
    };
    let plain = render_matchup(&view, HelpColorMode::Plain);
    assert!(!plain.contains("CATEGORIES"));
    assert!(plain.contains("8/20   6   2   8    1   .400"));
    assert!(plain.contains("11.0   1   1   9  6.55  1.55"));
    assert!(plain.find("PITCHER").unwrap() < plain.find("SUMMARY").unwrap());
    assert!(plain.contains("MY ODDS"));
    assert!(!plain.contains("Games:"));

    let colored = render_matchup(&view, HelpColorMode::Color);
    assert!(colored.contains("\u{1b}[1;38;5;34m   6\u{1b}[0m"));
    assert!(colored.contains("\u{1b}[1;38;5;196m  6.55\u{1b}[0m"));
    assert!(colored.contains("\u{1b}[1;38;5;231m   2\u{1b}[0m"));
    assert!(colored.contains("\u{1b}[38;5;34m██████░░░░ 55%\u{1b}[0m"));
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
            teams: vec![],
            stale: true,
            odds: vec![],
        },
        HelpColorMode::Plain,
    );
    assert!(output.contains("Operators (5-4-1 | —)"));
    assert!(!output.contains('💎'));
    assert!(!output.contains('⚾'));

    let local = render_local_matchup(
        &LocalMatchupView {
            team_name: "💎 Operators".into(),
            players: vec![],
        },
        HelpColorMode::Plain,
    );
    assert!(local.starts_with("YAHOO UNAVAILABLE"));
    assert!(local.contains("ROSTER: Operators"));
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
            day: Some("Jul-01".into()),
            ..MatchupOptions::default()
        }
        .validate()
        .is_ok()
    );
    assert!(
        MatchupOptions {
            week: Some(2),
            day: Some("jul-01".into()),
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
    assert!(
        MatchupOptions {
            day: Some("2026-07-01".into()),
            ..MatchupOptions::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        MatchupOptions {
            week: Some(2),
            ..MatchupOptions::default()
        }
        .validate()
        .is_ok()
    );
}

#[test]
fn local_fallback_omits_opponent_categories() {
    let output = render_local_matchup(
        &LocalMatchupView {
            team_name: "Operators".into(),
            players: vec![stored_hitter()],
        },
        HelpColorMode::Plain,
    );
    assert!(output.contains("showing local roster"));
    assert!(output.contains("SLOT  HITTER"));
    assert!(output.contains("Ada Hitter NYY"));
    assert!(!output.contains("CATEGORIES"));
    assert!(!output.contains("|"));
}
