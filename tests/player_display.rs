use std::collections::HashMap;

use b9::domain::{MatchupTeam, PlayerGameLog, StoredFantasyPlayer};
use b9::player_display::{render_detail, render_weekly_totals};
use b9::store::StoredFantasyCategory;
use b9::terminal::HelpColorMode;

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
