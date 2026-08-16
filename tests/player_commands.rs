use b9::domain::StoredFantasyPlayer;
use b9::player_commands::{waiver_eligible, with_yahoo_result_notice};
use b9::store::WaiverCandidate;

fn player(id: i64, role: &str, positions: &str) -> StoredFantasyPlayer {
    StoredFantasyPlayer {
        yahoo_player_id: Some(id),
        mlbam_id: Some(id),
        name: "Available".into(),
        team: "NYY".into(),
        role: role.into(),
        positions: positions.into(),
        status: String::new(),
        rank: None,
        percent_owned: None,
        owner: None,
        slot: None,
        batting: [0.0; 7],
        pitching: [0.0; 7],
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
fn yahoo_results_are_attributed_only_when_fresh() {
    assert!(with_yahoo_result_notice(false, "POOL\n".into()).contains("Fantasy data provided"));
    assert!(with_yahoo_result_notice(true, "POOL\n".into()).starts_with("STALE —"));
}
#[test]
fn evaluation_uses_name_as_a_stable_tie_breaker() {
    let mut players = vec![player(1, "B", "OF"), player(2, "B", "OF")];
    players[0].name = "Zed".into();
    players[1].name = "Ada".into();
    b9::evaluation::sort_by_evaluation(&mut players);
    assert_eq!(players[0].name, "Ada");
}
