use skout::domain::{GameIndicator, StoredFantasyPlayer};
use skout::player_commands::{waiver_eligible, with_yahoo_result_notice};
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
