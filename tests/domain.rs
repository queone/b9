use std::collections::HashMap;

use skout::domain::{
    BattingStats, GameLogRow, League, Matchup, MatchupTeam, PitchingStats, Player, PlayerWeekStats,
    Position, Roster, RosterWeekStats, ScoringType, StatcastData, clean_fantasy_team_name,
};

#[test]
fn fantasy_team_names_drop_emoji_without_damaging_text() {
    assert_eq!(
        clean_fantasy_team_name("💎 Jeff’s Finest Team"),
        "Jeff’s Finest Team"
    );
    assert_eq!(clean_fantasy_team_name("Los Niños ⚾"), "Los Niños");
}

#[test]
fn mlb_utility_records_round_trip_without_provider_fields() {
    let team = skout::domain::MlbTeam {
        id: 147,
        name: "New York Yankees".into(),
        location: "New York".into(),
        club_name: "Yankees".into(),
        abbreviation: "NYY".into(),
        league_id: 103,
    };
    let encoded = serde_json::to_string(&team).unwrap();
    assert_eq!(
        serde_json::from_str::<skout::domain::MlbTeam>(&encoded).unwrap(),
        team
    );
    let row = skout::domain::MlbSlateRow {
        date: "2026-08-15".into(),
        game_id: 1,
        game_time: "2026-08-15 19:05".into(),
        away_team: "NYY".into(),
        home_team: "BOS".into(),
        away_pitcher: "Starter".into(),
        home_pitcher: "Other Starter".into(),
        win_probability: Some(0.55),
        away_free_agent: false,
        home_free_agent: false,
        away_mine: false,
        home_mine: false,
    };
    assert_eq!(
        serde_json::from_str::<skout::domain::MlbSlateRow>(&serde_json::to_string(&row).unwrap())
            .unwrap(),
        row
    );
}

fn batting_stats() -> BattingStats {
    BattingStats {
        plate_appearances: 101,
        batting_average: 0.201,
        on_base_percentage: 0.302,
        slugging_percentage: 0.403,
        on_base_plus_slugging: 0.705,
        home_runs: 6,
        runs_batted_in: 7,
        runs: 8,
        stolen_bases: 9,
        strikeouts: 10,
        walks: 11,
    }
}

fn pitching_stats() -> PitchingStats {
    PitchingStats {
        games: 12,
        games_started: 13,
        innings_pitched: 14.1,
        earned_run_average: 2.15,
        whip: 1.16,
        strikeouts: 17,
        strikeouts_per_nine: 8.18,
        walks_per_nine: 2.19,
        fielding_independent_pitching: 3.20,
        expected_fielding_independent_pitching: 3.21,
        wins: 22,
        saves: 23,
        holds: 24,
        quality_starts: 25,
        rate_strikeouts: 26,
        walks: 27,
        batters_faced: 28,
    }
}

fn statcast_data() -> StatcastData {
    StatcastData {
        average_exit_velocity: 80.01,
        barrel_percentage: 2.02,
        hard_hit_percentage: 3.03,
        expected_batting_average: 0.204,
        expected_slugging_percentage: 0.405,
        expected_weighted_on_base_average: 0.306,
        average_launch_angle: 7.07,
        sweet_spot_percentage: 8.08,
        sprint_speed: 29.09,
        fly_ball_percentage: 10.10,
        home_run_to_fly_ball_percentage: 11.11,
        fastball_velocity: 92.12,
        spin_rate: 2313.0,
        whiff_percentage: 14.14,
        chase_percentage: 15.15,
        pitching_hard_hit_percentage: 16.16,
        ground_ball_percentage: 17.17,
        pitching_fly_ball_percentage: 18.18,
        expected_earned_run_average: 3.19,
        expected_fielding_independent_pitching: 3.20,
        plate_appearances: 321,
        batted_ball_events: 222,
    }
}

fn player(name: &str, roster_position: Position, batting: bool, pitching: bool) -> Player {
    Player {
        id: 1,
        yahoo_player_key: "423.p.2".to_owned(),
        mlb_player_id: 3,
        name: name.to_owned(),
        team: "NYY".to_owned(),
        positions: vec![Position::Outfield, Position::Utility],
        bat_side: "L".to_owned(),
        pitch_hand: "R".to_owned(),
        birth_date: "2000-08-15".to_owned(),
        jersey_number: "4".to_owned(),
        roster_position,
        injury_status: "Active".to_owned(),
        injury_note: "note-a".to_owned(),
        mlbam_injury_note: "note-b".to_owned(),
        ownership_percentage: 5.5,
        ownership_delta: 6.6,
        percentage_started: 7.7,
        yahoo_rank: 8,
        batting: batting.then(batting_stats),
        pitching: pitching.then(pitching_stats),
        statcast_raw: Some(statcast_data()),
        statcast_blended: Some(statcast_data()),
        primary_type: "independent".to_owned(),
        player_quality_score: 9.9,
        is_closer: true,
        spring_only: false,
        projected_production: 10,
        is_recent_callup: true,
        expert_consensus_rank: 11,
        fangraphs_war: 12.2,
        weighted_runs_created_plus: 13,
        owner: "My Team".to_owned(),
    }
}

fn week_player(name: &str, position_type: &str) -> PlayerWeekStats {
    PlayerWeekStats {
        yahoo_player_id: 101,
        name: name.to_owned(),
        team: "BOS".to_owned(),
        position_type: position_type.to_owned(),
        slot_position: Position::Utility,
        eligible_positions: vec![Position::FirstBase, Position::Utility],
        injury_status: "DTD".to_owned(),
        hab: "2/8".to_owned(),
        runs: 2,
        home_runs: 3,
        runs_batted_in: 4,
        stolen_bases: 5,
        batting_average: ".250".to_owned(),
        innings_pitched: "6.1".to_owned(),
        wins: 6,
        saves: 7,
        strikeouts: 8,
        earned_run_average: "2.70".to_owned(),
        whip: "0.99".to_owned(),
    }
}

#[test]
fn compatibility_values_round_trip_losslessly() {
    for value in ["rotisserie", "head-to-head", "points", "custom-score"] {
        assert_eq!(ScoringType::from(value).to_string(), value);
    }
    for value in [
        "C", "1B", "2B", "3B", "SS", "OF", "SP", "RP", "Util", "BN", "IL", "IL+",
    ] {
        assert_eq!(Position::from(value).to_string(), value);
    }
    assert_eq!(
        ScoringType::from("Points"),
        ScoringType::Other("Points".to_owned())
    );
    assert_eq!(Position::from("util"), Position::Other("util".to_owned()));
}

#[test]
fn field_complete_records_preserve_distinct_values() {
    let league = League {
        league_key: "league-key".to_owned(),
        name: "League Name".to_owned(),
        season: 2026,
        num_teams: 12,
        scoring_type: ScoringType::HeadToHead,
        roster_positions: vec![Position::Catcher, Position::Bench],
        batting_categories: vec!["R".to_owned()],
        pitching_categories: vec!["ERA".to_owned()],
    };
    let mut stats = HashMap::new();
    stats.insert("H/AB".to_owned(), "12/45".to_owned());
    let team_a = MatchupTeam {
        team_key: "team-a".to_owned(),
        team_id: 1,
        name: "A".to_owned(),
        is_current_login: true,
        stats,
        wins: 2,
        losses: 3,
        ties: 4,
        completed_games: 5,
        live_games: 6,
        remaining_games: 7,
    };
    let team_b = MatchupTeam {
        team_key: "team-b".to_owned(),
        team_id: 8,
        name: "B".to_owned(),
        is_current_login: false,
        stats: HashMap::new(),
        wins: 9,
        losses: 10,
        ties: 11,
        completed_games: 12,
        live_games: 13,
        remaining_games: 14,
    };
    let matchup = Matchup {
        week: 15,
        week_start: "2026-08-10".to_owned(),
        week_end: "2026-08-16".to_owned(),
        status: "midevent".to_owned(),
        teams: [team_a.clone(), team_b],
    };
    let week = RosterWeekStats {
        team_key: "week-key".to_owned(),
        team_name: "Week Team".to_owned(),
        week: 16,
        players: vec![week_player("Batter", "B"), week_player("Pitcher", "P")],
    };
    let game = GameLogRow {
        date: "Aug 15".to_owned(),
        opponent_abbreviation: "TOR".to_owned(),
        is_home: true,
        team_result: "W, 4-3".to_owned(),
        batting_order: 1,
        hab: "2/4".to_owned(),
        runs: 2,
        home_runs: 3,
        runs_batted_in: 4,
        stolen_bases: 5,
        batting_average: 0.5,
        innings_pitched_decimal: 6.2,
        wins: 7,
        saves: 8,
        strikeouts: 9,
        earned_run_average: 1.1,
        whip: 0.9,
    };
    let roster = Roster {
        league_key: "roster-league".to_owned(),
        season: "2026".to_owned(),
        team_key: "roster-team".to_owned(),
        team_name: "Roster Team".to_owned(),
        players: vec![player("Complete Player", Position::Utility, true, true)],
    };

    assert_eq!(league.roster_positions[1], Position::Bench);
    assert_eq!(matchup.teams[0].stats["H/AB"], "12/45");
    assert_eq!(week.players[1].strikeouts, 8);
    assert_eq!(batting_stats().walks, 11);
    assert_eq!(pitching_stats().batters_faced, 28);
    assert_eq!(statcast_data().batted_ball_events, 222);
    assert_eq!(game.opponent_abbreviation, "TOR");
    assert_eq!(roster.players[0].mlbam_injury_note, "note-b");
}

#[test]
fn matchup_and_weekly_filters_match_source_behavior() {
    let team = MatchupTeam {
        team_key: "key".to_owned(),
        team_id: 1,
        name: "name".to_owned(),
        is_current_login: true,
        stats: HashMap::new(),
        wins: 7,
        losses: 3,
        ties: 1,
        completed_games: 4,
        live_games: 2,
        remaining_games: 5,
    };
    assert_eq!(team.score(), 7);
    assert_eq!(team.total_games(), 11);

    let week = RosterWeekStats {
        team_key: "key".to_owned(),
        team_name: "name".to_owned(),
        week: 1,
        players: vec![
            week_player("B1", "B"),
            week_player("P1", "P"),
            week_player("Unknown", "b"),
            week_player("B2", "B"),
        ],
    };
    assert_eq!(
        week.batters()
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["B1", "B2"]
    );
    assert_eq!(
        week.pitchers()
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["P1"]
    );
}

#[test]
fn player_roles_eligibility_and_age_are_deterministic() {
    let both = player("Both", Position::Utility, true, true);
    assert!(both.is_batter());
    assert!(both.is_pitcher());
    assert!(both.eligible_at(&Position::Outfield));
    assert!(!both.eligible_at(&Position::Catcher));
    assert_eq!(both.primary_type, "independent");

    assert_eq!(both.age_on(2026, 8, 14), Some(25));
    assert_eq!(both.age_on(2026, 8, 15), Some(26));
    assert_eq!(both.age_on(2026, 8, 16), Some(26));
    assert_eq!(both.age_on(2026, 2, 29), None);

    let mut leap = player("Leap", Position::Utility, true, false);
    leap.birth_date = "2000-02-29".to_owned();
    assert_eq!(leap.age_on(2024, 2, 29), Some(24));
    assert_eq!(leap.age_on(2023, 2, 28), Some(22));
    assert_eq!(leap.age_on(2023, 3, 1), Some(23));

    for invalid in ["", "2000-2-29", "2001-02-29", "2000-13-01", "abcd-ef-gh"] {
        leap.birth_date = invalid.to_owned();
        assert_eq!(leap.age_on(2026, 8, 15), None);
    }
    leap.birth_date = "2030-01-01".to_owned();
    assert_eq!(leap.age_on(2026, 8, 15), None);
    assert_eq!(both.age_on(0, 1, 1), None);
    assert_eq!(both.age_on(2026, 13, 1), None);
}

#[test]
fn roster_filters_and_unicode_lookup_preserve_order() {
    let roster = Roster {
        league_key: "league".to_owned(),
        season: "2026".to_owned(),
        team_key: "team".to_owned(),
        team_name: "Team".to_owned(),
        players: vec![
            player("ÉLODIE", Position::Utility, true, false),
            player("Bench", Position::Bench, true, false),
            player("IL", Position::InjuredList, false, true),
            player("Pitcher", Position::StartingPitcher, false, true),
        ],
    };

    assert_eq!(
        roster
            .active_players()
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["ÉLODIE", "Pitcher"]
    );
    assert_eq!(
        roster
            .batters()
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["ÉLODIE", "Bench"]
    );
    assert_eq!(
        roster
            .pitchers()
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["IL", "Pitcher"]
    );
    assert_eq!(
        roster.player_by_name("élodie").map(|p| p.name.as_str()),
        Some("ÉLODIE")
    );
    assert!(roster.player_by_name("ÉLOD").is_none());
}

#[test]
fn only_statistics_records_have_zero_defaults() {
    assert_eq!(BattingStats::default().plate_appearances, 0);
    assert_eq!(PitchingStats::default().innings_pitched, 0.0);
    assert_eq!(StatcastData::default().batted_ball_events, 0);
}
