use b9::providers::yahoo_fantasy::{
    bounded_page_starts, parse_free_agents, parse_league_rosters, parse_league_settings,
    parse_roster_week_stats, parse_scoreboard, parse_standings, parse_user_leagues,
};

fn fixture(name: &str) -> serde_json::Value {
    let bytes = match name {
        "leagues" => include_bytes!("fixtures/yahoo/leagues.json").as_slice(),
        "settings" => include_bytes!("fixtures/yahoo/league-settings.json").as_slice(),
        "standings" => include_bytes!("fixtures/yahoo/standings.json").as_slice(),
        "rosters" => include_bytes!("fixtures/yahoo/rosters.json").as_slice(),
        "matchup" => include_bytes!("fixtures/yahoo/matchup.json").as_slice(),
        "weekly" => include_bytes!("fixtures/yahoo/weekly-stats.json").as_slice(),
        _ => unreachable!(),
    };
    serde_json::from_slice(bytes).unwrap()
}

#[test]
fn selected_fixtures_decode_complete_workflow_records() {
    let leagues = parse_user_leagues(&fixture("leagues")).unwrap();
    assert_eq!(leagues[0].team_key, "mlb.l.1.t.1");
    let settings = parse_league_settings("mlb.l.1", &fixture("settings")).unwrap();
    assert_eq!(settings.current_week, Some(7));
    assert_eq!(settings.categories.len(), 2);
    assert_eq!(settings.roster_positions.len(), 2);
    let teams = parse_standings("mlb.l.1", &fixture("standings")).unwrap();
    assert_eq!(teams.len(), 2);
    assert_eq!((teams[0].rank, teams[0].wins, teams[0].moves), (1, 107, 29));
    assert_eq!(teams[1].name, "Opponents");
    let rosters = parse_league_rosters("mlb.l.1", &fixture("rosters")).unwrap();
    assert_eq!((rosters.players.len(), rosters.slots.len()), (2, 2));
    let matchup = parse_scoreboard(&fixture("matchup")).unwrap();
    assert_eq!(matchup[0].week, 7);
    assert_eq!(matchup[0].teams[0].stats["7"], "12");
    let weekly = parse_roster_week_stats("mlb.l.1.t.1", 7, &fixture("weekly")).unwrap();
    assert_eq!(weekly.players[0].home_runs, 1);
}

#[test]
fn empty_and_incomplete_complete_snapshots_are_explicit() {
    let empty = serde_json::json!({"data": []});
    assert!(parse_standings("mlb.l.1", &empty).is_err());
    assert!(parse_league_rosters("mlb.l.1", &empty).is_err());
    assert!(parse_scoreboard(&empty).unwrap().is_empty());
    assert!(parse_roster_week_stats("mlb.l.1.t.1", 7, &empty).is_err());
}

#[test]
fn pagination_offsets_are_bounded() {
    assert_eq!(bounded_page_starts(51, 25).unwrap(), vec![0, 25, 50]);
    assert!(bounded_page_starts(1, 0).is_err());
    assert!(bounded_page_starts(501, 25).is_err());
}

#[test]
fn free_agent_page_retains_ranked_players() {
    let page = serde_json::from_slice(include_bytes!("fixtures/yahoo/free-agents.json")).unwrap();
    let players = parse_free_agents(&page).unwrap();
    assert_eq!(players.len(), 2);
    assert_eq!(players[0].name, "Ada Available");
    assert_eq!(players[1].position_type, "P");
}

#[test]
fn roster_rank_selection_prefers_actual_season_values() {
    let response = serde_json::json!({"data": [{
        "team_key": "mlb.l.1.t.1",
        "players": [
            {
                "player_id": 101,
                "full": "Previous Actual",
                "position_type": "B",
                "display_position": "OF",
                "position": "OF",
                "player_ranks": [
                    {"player_rank": {"rank_type": "OR", "rank_value": "22"}},
                    {"player_rank": {"rank_season": "2026", "rank_type": "S", "rank_value": "22"}},
                    {"player_rank": {"rank_season": "2025", "rank_type": "S", "rank_value": "321"}}
                ]
            },
            {
                "player_id": 202,
                "full": "Current Actual",
                "position_type": "P",
                "display_position": "SP",
                "position": "SP",
                "player_ranks": [
                    {"player_rank": {"rank_type": "OR", "rank_value": "12"}},
                    {"player_rank": {"rank_season": "2026", "rank_type": "S", "rank_value": "44"}},
                    {"player_rank": {"rank_season": "2025", "rank_type": "S", "rank_value": "90"}}
                ]
            }
        ]
    }]});

    let roster = parse_league_rosters("mlb.l.1", &response).unwrap();
    assert_eq!(roster.players[0].yahoo_rank, Some(321));
    assert_eq!(roster.players[1].yahoo_rank, Some(44));
}

#[test]
fn league_roster_excludes_players_yahoo_marks_as_dropped() {
    let response = serde_json::json!({"data": [{
        "team_key": "mlb.l.1.t.1",
        "players": [
            {
                "player_id": 101,
                "full": "Active Player",
                "position_type": "P",
                "display_position": "SP",
                "position": "SP"
            },
            {
                "player_id": 202,
                "full": "Dropped Player",
                "position_type": "P",
                "display_position": "SP",
                "position": "--"
            }
        ]
    }]});

    let roster = parse_league_rosters("mlb.l.1", &response).unwrap();

    assert_eq!(roster.players.len(), 1);
    assert_eq!(roster.players[0].name, "Active Player");
    assert_eq!(roster.slots.len(), 1);
    assert_eq!(roster.slots[0].yahoo_player_id, 101);
}
