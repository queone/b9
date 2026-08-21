use skout::domain::Position;
use skout::providers::yahoo_fantasy::{
    bounded_page_starts, parse_free_agents, parse_league_rosters, parse_league_settings,
    parse_roster_week_stats, parse_scoreboard, parse_standings, parse_team_rosters,
    parse_user_leagues,
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
fn parse_team_rosters_merges_per_team_responses_and_injects_missing_team_key() {
    // Team 1's response echoes team_key; team 2's does not, matching a real
    // single-team Yahoo response that may omit it — the caller-supplied key
    // must win either way.
    let team_1 = serde_json::json!({"data": {
        "team_key": "mlb.l.1.t.1",
        "players": [
            {"player_id": 101, "full": "Ada Hitter", "display_position": "OF", "position": "OF"}
        ]
    }});
    let team_2 = serde_json::json!({"data": {
        "players": [
            {"player_id": 202, "full": "Grace Pitcher", "display_position": "SP", "position": "SP"}
        ]
    }});
    let rosters = parse_team_rosters(&[
        ("mlb.l.1.t.1".to_string(), team_1),
        ("mlb.l.1.t.2".to_string(), team_2),
    ])
    .unwrap();
    assert_eq!(rosters.players.len(), 2);
    assert_eq!(rosters.slots.len(), 2);
    assert!(
        rosters
            .slots
            .iter()
            .any(|slot| slot.team_key == "mlb.l.1.t.2" && slot.yahoo_player_id == 202)
    );
}

#[test]
fn parse_team_rosters_handles_yahoo_real_array_of_single_key_field_shape() {
    // Yahoo's real (non-simplified) wire shape encodes one entity as
    // `[fields, {named_subresource}, ...]`, where `fields` is itself an
    // array of 1-key objects — confirmed against live production Yahoo
    // responses, never reflected in this repo's other (hand-simplified)
    // fixtures. `selected_position` nests the roster slot two levels below
    // `player`, and `is_keeper.status` deliberately collides with the
    // player-level `status` (injury designator) field name to prove that
    // collision doesn't shadow a real injury status when one exists.
    let player_fields = serde_json::json!([
        {"player_id": 501},
        {"name": {"full": "Ada Hitter"}},
        {"status": "DTD"},
        {"editorial_team_abbr": "NYY"},
        {"is_keeper": {"status": false, "cost": false, "kept": false}},
        {"display_position": "OF"}
    ]);
    let player = serde_json::json!([
        player_fields,
        {"selected_position": [{"coverage_type": "date"}, {"position": "OF"}]}
    ]);
    let roster = serde_json::json!({
        "roster": {"0": {"players": {"0": {"player": player}}}}
    });
    let team_fields = serde_json::json!([
        {"team_key": "mlb.l.1.t.1"},
        {"name": "Testers"}
    ]);
    let team = serde_json::json!({"data": {"team": [team_fields, roster]}});
    let rosters = parse_team_rosters(&[("mlb.l.1.t.1".to_string(), team)]).unwrap();
    assert_eq!(rosters.players.len(), 1);
    let player = &rosters.players[0];
    assert_eq!(player.yahoo_player_id, 501);
    assert_eq!(player.name, "Ada Hitter");
    assert_eq!(player.injury_status, "DTD");
    assert_eq!(rosters.slots[0].slot_position, Position::from("OF"));
}

#[test]
fn parse_team_rosters_resolves_selected_position_not_first_eligible_position() {
    // A real player's `eligible_positions` (multiple positions they could
    // play) and `selected_position` (their one actual assigned slot) both
    // use the field name `position`. `eligible_positions` appears earlier
    // in wire order; first-wins hoisting must not let its first entry
    // shadow the real assigned slot from `selected_position`.
    let player_fields = serde_json::json!([
        {"player_id": 501},
        {"name": {"full": "Ada Bencher"}},
        {"display_position": "1B,OF"},
        {"eligible_positions": [
            {"position": "1B"},
            {"position": "OF"},
            {"position": "Util"}
        ]},
        {"eligible_positions_to_add": []}
    ]);
    let player = serde_json::json!([
        player_fields,
        {"selected_position": [{"coverage_type": "date"}, {"position": "BN"}]}
    ]);
    let roster = serde_json::json!({
        "roster": {"0": {"players": {"0": {"player": player}}}}
    });
    let team_fields = serde_json::json!([
        {"team_key": "mlb.l.1.t.1"},
        {"name": "Testers"}
    ]);
    let team = serde_json::json!({"data": {"team": [team_fields, roster]}});
    let rosters = parse_team_rosters(&[("mlb.l.1.t.1".to_string(), team)]).unwrap();
    assert_eq!(rosters.slots.len(), 1);
    assert_eq!(rosters.slots[0].slot_position, Position::from("BN"));
}

#[test]
fn parse_team_rosters_leaves_injury_status_blank_when_is_keeper_is_the_only_status_field() {
    // Without a real player-level `status`, `is_keeper.status` (false) must
    // not leak through as a bogus "false" injury status.
    let player_fields = serde_json::json!([
        {"player_id": 501},
        {"name": {"full": "Ada Hitter"}},
        {"is_keeper": {"status": false, "cost": false, "kept": false}},
        {"display_position": "OF"}
    ]);
    let player = serde_json::json!([
        player_fields,
        {"selected_position": [{"coverage_type": "date"}, {"position": "OF"}]}
    ]);
    let roster = serde_json::json!({
        "roster": {"0": {"players": {"0": {"player": player}}}}
    });
    let team_fields = serde_json::json!([
        {"team_key": "mlb.l.1.t.1"},
        {"name": "Testers"}
    ]);
    let team = serde_json::json!({"data": {"team": [team_fields, roster]}});
    let rosters = parse_team_rosters(&[("mlb.l.1.t.1".to_string(), team)]).unwrap();
    assert_eq!(rosters.players[0].injury_status, "");
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
fn historical_roster_infers_missing_player_roles_from_display_position() {
    let value = serde_json::json!({"data": [{
        "team_key": "mlb.l.1.t.1",
        "players": [
            {"player_id": 1, "full": "Ada Hitter", "display_position": "OF", "position": "OF",
             "player_stats": {"stats": [{"stat_id": 6, "value": "4"}, {"stat_id": 8, "value": "2"}, {"stat_id": 12, "value": "1"}, {"stat_id": 3, "value": ".500"}]}},
            {"player_id": 2, "full": "Grace Pitcher", "display_position": "SP,RP", "position": "BN"},
            {"player_id": 3, "full": "Unknown Role", "position": "BN"}
        ]
    }]});
    let roster = parse_roster_week_stats("mlb.l.1.t.1", 7, &value).unwrap();
    assert_eq!(roster.players[0].position_type, "B");
    assert_eq!(roster.players[0].hab, "2-4");
    assert_eq!(roster.players[0].home_runs, 1);
    assert_eq!(roster.players[0].batting_average, ".500");
    assert_eq!(roster.players[1].position_type, "P");
    assert!(roster.players[2].position_type.is_empty());
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
