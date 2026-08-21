use skout::domain::{GameIndicator, StoredFantasyPlayer};
use skout::player_commands::{
    player_pool_limit, sort_waiver_players, waiver_eligible, with_yahoo_result_notice,
    yahoo_pickup_available,
};
use skout::store::WaiverCandidate;

fn player(id: i64, role: &str, positions: &str) -> StoredFantasyPlayer {
    StoredFantasyPlayer {
        yahoo_player_id: Some(id),
        mlbam_id: Some(id),
        name: "Available".into(),
        team: "NYY".into(),
        role: role.into(),
        positions: positions.into(),
        is_closer: false,
        status: String::new(),
        injury_note: String::new(),
        birth_date: String::new(),
        game_status: String::new(),
        game_indicator: GameIndicator::None,
        hand: String::new(),
        rank: None,
        percent_owned: None,
        percentage_started: 0.0,
        expert_consensus_rank: None,
        owner: None,
        slot: None,
        batting: [0.0; 7],
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

#[test]
fn waiver_gate_uses_active_role_and_position_percentiles() {
    let candidates = vec![
        WaiverCandidate {
            mlbam_id: 1,
            role: "H".into(),
            positions: "C".into(),
            plate_appearances: 30.0,
            innings_pitched: 0.0,
            games: 0,
            games_started: 0,
        },
        WaiverCandidate {
            mlbam_id: 2,
            role: "H".into(),
            positions: "C".into(),
            plate_appearances: 60.0,
            innings_pitched: 0.0,
            games: 0,
            games_started: 0,
        },
        WaiverCandidate {
            mlbam_id: 3,
            role: "H".into(),
            positions: "C".into(),
            plate_appearances: 90.0,
            innings_pitched: 0.0,
            games: 0,
            games_started: 0,
        },
    ];
    assert!(!waiver_eligible(&player(1, "B", "C"), None, &candidates));
    assert!(waiver_eligible(
        &player(3, "B", "C"),
        Some("C"),
        &candidates
    ));
}

#[test]
fn waiver_gate_excludes_injured_and_off_roster_players() {
    let candidates = vec![WaiverCandidate {
        mlbam_id: 7,
        role: "P".into(),
        positions: "SP".into(),
        plate_appearances: 0.0,
        innings_pitched: 12.0,
        games: 2,
        games_started: 2,
    }];
    let mut injured = player(7, "P", "SP");
    injured.status = "IL10".into();
    assert!(!waiver_eligible(&injured, None, &candidates));
    let mut owned = player(7, "P", "SP");
    owned.owner = Some("Operators".into());
    assert!(!waiver_eligible(&owned, None, &candidates));
    assert!(!waiver_eligible(&player(8, "P", "SP"), None, &candidates));
}

#[test]
fn yahoo_results_show_only_the_stale_notice() {
    assert_eq!(with_yahoo_result_notice(false, "POOL\n".into()), "POOL\n");
    assert!(with_yahoo_result_notice(true, "POOL\n".into()).starts_with("STALE —"));
}
#[test]
fn evaluation_uses_name_as_a_stable_tie_breaker() {
    let mut players = vec![player(1, "B", "OF"), player(2, "B", "OF")];
    players[0].name = "Zed".into();
    players[1].name = "Ada".into();
    skout::analysis::pqs::sort_by_pqs(&mut players);
    assert_eq!(players[0].name, "Ada");
}

#[test]
fn yahoo_availability_includes_low_data_and_injured_players() {
    for role in ["B", "P"] {
        let mut missing_identity = player(10, role, if role == "B" { "OF" } else { "SP" });
        missing_identity.mlbam_id = None;
        assert!(yahoo_pickup_available(&missing_identity));

        let below_usage_floor = player(11, role, if role == "B" { "OF" } else { "RP" });
        assert!(yahoo_pickup_available(&below_usage_floor));

        for status in ["IL10", "NA", "SUSP"] {
            let mut unavailable_to_play = player(12, role, if role == "B" { "OF" } else { "SP" });
            unavailable_to_play.status = status.into();
            assert!(yahoo_pickup_available(&unavailable_to_play));
        }
    }
}

#[test]
fn yahoo_availability_excludes_owned_players() {
    let mut owned = player(20, "B", "OF");
    owned.owner = Some("Operators".into());
    assert!(!yahoo_pickup_available(&owned));
}

#[test]
fn waiver_sort_keeps_qualified_players_first_and_orders_fallbacks() {
    let candidates = vec![
        WaiverCandidate {
            mlbam_id: 1,
            role: "H".into(),
            positions: "OF".into(),
            plate_appearances: 100.0,
            innings_pitched: 0.0,
            games: 0,
            games_started: 0,
        },
        WaiverCandidate {
            mlbam_id: 2,
            role: "H".into(),
            positions: "OF".into(),
            plate_appearances: 90.0,
            innings_pitched: 0.0,
            games: 0,
            games_started: 0,
        },
        WaiverCandidate {
            mlbam_id: 3,
            role: "H".into(),
            positions: "OF".into(),
            plate_appearances: 10.0,
            innings_pitched: 0.0,
            games: 0,
            games_started: 0,
        },
    ];
    let mut first = player(1, "B", "OF");
    first.name = "Zed Qualified".into();
    let mut second = player(2, "B", "OF");
    second.name = "Ada Qualified".into();
    second.mlbam_id = Some(1);
    let mut ranked_fallback = player(3, "B", "OF");
    ranked_fallback.name = "Zed Fallback".into();
    ranked_fallback.rank = Some(1);
    let mut tied_fallback = player(4, "B", "OF");
    tied_fallback.name = "Ada Fallback".into();
    tied_fallback.rank = Some(1);
    tied_fallback.mlbam_id = None;
    let mut expected_qualified = vec![first.clone(), second.clone()];
    skout::analysis::pqs::sort_by_pqs(&mut expected_qualified);
    let mut players = vec![ranked_fallback, first, tied_fallback, second];

    sort_waiver_players(&mut players, None, &candidates);

    assert_eq!(
        players[..2]
            .iter()
            .map(|player| player.name.as_str())
            .collect::<Vec<_>>(),
        expected_qualified
            .iter()
            .map(|player| player.name.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(players[2].name, "Ada Fallback");
    assert_eq!(players[3].name, "Zed Fallback");
}

#[test]
fn player_pool_limit_preserves_default_and_numeric_expansion() {
    assert_eq!(player_pool_limit(None), 20);
    assert_eq!(player_pool_limit(Some("50")), 50);
    assert_eq!(player_pool_limit(Some("player name")), 20);
}
