use std::collections::HashMap;

use b9::domain::{MatchupTeam, PlayerGameLog, StoredFantasyPlayer};
use b9::player_display::{
    render_detail, render_league_totals, render_players, render_weekly_totals,
};
use b9::store::{StoredFantasyCategory, StoredFantasyTeam};
use b9::terminal::{HelpColorMode, visible_width};

fn hitter() -> StoredFantasyPlayer {
    StoredFantasyPlayer {
        yahoo_player_id: Some(1),
        mlbam_id: Some(2),
        name: "Ada Hitter".into(),
        team: "NYY".into(),
        role: "B".into(),
        positions: "OF".into(),
        status: String::new(),
        rank: Some(4),
        percent_owned: Some(99.0),
        owner: None,
        slot: None,
        batting: [10.0, 0.3, 2.0, 1.0, 3.0, 1.0, 0.25],
        pitching: [0.0; 7],
    }
}

#[test]
fn detail_renders_recent_game_log_and_stale_label() {
    let output = render_detail(
        &hitter(),
        &serde_json::from_slice::<Vec<PlayerGameLog>>(include_bytes!(
            "fixtures/player/game-log.json"
        ))
        .unwrap(),
        true,
        HelpColorMode::Plain,
    );
    assert!(output.contains("GAME LOG data may be stale"));
    assert!(output.contains("2026-04-01  @ BOS  AB 4"));
}

#[test]
fn weekly_totals_follow_league_category_order() {
    let output = render_weekly_totals(
        "Operators",
        "WEEK 7",
        &MatchupTeam {
            team_key: "mlb.l.1.t.1".into(),
            team_id: 1,
            name: "Operators".into(),
            is_current_login: true,
            stats: HashMap::from([("7".into(), "12".into()), ("60".into(), ".321".into())]),
            wins: 0,
            losses: 0,
            ties: 0,
            completed_games: 0,
            live_games: 0,
            remaining_games: 0,
        },
        &[
            StoredFantasyCategory {
                stat_id: 60,
                abbreviation: "AVG".into(),
                sequence: 1,
            },
            StoredFantasyCategory {
                stat_id: 7,
                abbreviation: "R".into(),
                sequence: 2,
            },
        ],
        true,
        HelpColorMode::Plain,
    );
    assert!(output.contains("STALE — showing the last complete Yahoo weekly snapshot."));
    assert!(output.contains("AVG    .321\nR      12"));
    assert!(!output.contains("Fantasy data provided by Yahoo Fantasy"));
}

#[test]
fn player_table_matches_skout_column_shape_without_deferred_signals() {
    let output = render_players("HITTERS", &[hitter()], HelpColorMode::Plain);
    assert!(output.contains("HITTER"));
    assert!(output.contains("POS"));
    assert!(output.contains("STATUS"));
    assert!(output.contains("YR"));
    assert!(output.contains("PA"));
    assert!(output.contains("OBP"));
    assert!(output.contains("OWNER"));
    assert!(output.contains("Ada Hitter NYY"));
    assert!(output.contains("<available>"));
    assert!(!output.contains("SCORE"));
    assert!(!output.contains("RATIONALE"));
    assert!(!output.contains("PQS"));
    assert!(!output.contains("PQT"));
    assert!(!output.contains("SHS"));
}

#[test]
fn player_table_preserves_skout_palette_and_visible_column_widths() {
    let mut available = hitter();
    available.slot = Some("OF".into());
    let mut bench = hitter();
    bench.name = "Benched Hitter".into();
    bench.slot = Some("BN".into());
    let mut injured = hitter();
    injured.name = "Injured Hitter".into();
    injured.slot = Some("IL".into());
    injured.status = "IL10".into();
    let players = [available, bench, injured];

    let plain = render_players("Operators", &players, HelpColorMode::Plain);
    let colored = render_players("Operators", &players, HelpColorMode::Color);
    assert!(colored.contains("\u{1b}[38;5;33mROSTER:\u{1b}[0m"));
    assert!(colored.contains("\u{1b}[38;5;34mOperators\u{1b}[0m"));
    assert!(colored.contains("\u{1b}[38;5;245mBN"));
    assert!(colored.contains("\u{1b}[38;5;100mIL"));
    assert_eq!(
        plain.lines().map(visible_width).collect::<Vec<_>>(),
        colored.lines().map(visible_width).collect::<Vec<_>>()
    );
}

#[test]
fn pitcher_pool_does_not_emit_an_empty_hitter_section() {
    let mut pitcher = hitter();
    pitcher.role = "P".into();
    pitcher.positions = "SP".into();
    pitcher.batting = [0.0; 7];
    pitcher.pitching = [10.0, 1.0, 2.0, 0.0, 12.0, 2.7, 1.1];
    let output = render_players("PITCHERS", &[pitcher], HelpColorMode::Plain);
    assert!(output.starts_with("PITCHER"));
    assert!(!output.contains("HITTER"));
}

#[test]
fn league_totals_match_the_skout_all_team_shape_and_weight_rates() {
    let teams = [
        StoredFantasyTeam {
            team_key: "one".into(),
            name: "Operators".into(),
            manager_name: "Ada".into(),
            team_id: 1,
            waiver_priority: 8,
            faab_balance: 65,
            wins: 107,
            losses: 44,
            ties: 9,
            moves: 29,
            rank: 1,
        },
        StoredFantasyTeam {
            team_key: "two".into(),
            name: "Opponents".into(),
            manager_name: "Grace".into(),
            team_id: 2,
            waiver_priority: 2,
            faab_balance: 33,
            wins: 85,
            losses: 64,
            ties: 11,
            moves: 56,
            rank: 2,
        },
    ];
    let mut first = hitter();
    first.owner = Some("Operators".into());
    first.batting = [100.0, 0.4, 10.0, 4.0, 12.0, 3.0, 0.3];
    let mut second = hitter();
    second.name = "Second Hitter".into();
    second.owner = Some("Operators".into());
    second.batting = [300.0, 0.2, 20.0, 6.0, 18.0, 7.0, 0.2];
    let plain = render_league_totals(&teams, &[first, second], HelpColorMode::Plain);
    let header = plain.lines().next().unwrap();
    for column in [
        "TEAM", "RANK", "WLT", "PCT", "GB", "LW", "BDGT", "WVR", "MOVES", "PA", "OBP", "R", "HR",
        "RBI", "SB", "AVG", "IP", "QS", "W", "SV", "K", "ERA", "WHIP",
    ] {
        assert!(header.contains(column), "missing {column}");
    }
    assert!(plain.contains("Operators"));
    assert!(plain.contains("107-44-9"));
    assert!(plain.contains("  400"));
    assert!(plain.contains(" .250"));
    assert!(plain.contains("   30"));
    assert!(plain.contains("   10"));
    assert_eq!(plain.lines().count(), 3);

    let color = render_league_totals(&teams, &[], HelpColorMode::Color);
    assert!(color.starts_with("\u{1b}[38;5;33mTEAM"));
    assert!(color.contains("\u{1b}[38;5;245m"));
}
