use b9::domain::StoredFantasyPlayer;
use b9::evaluation::{evaluate, sort_by_evaluation};

fn player(name: &str, role: &str, batting: [f64; 7], pitching: [f64; 7]) -> StoredFantasyPlayer {
    StoredFantasyPlayer {
        yahoo_player_id: None,
        mlbam_id: None,
        name: name.into(),
        team: String::new(),
        role: role.into(),
        positions: String::new(),
        status: String::new(),
        injury_note: String::new(),
        birth_date: String::new(),
        game_status: String::new(),
        hand: String::new(),
        rank: None,
        percent_owned: None,
        owner: None,
        slot: None,
        batting,
        pitching,
        hitting_advanced: [None; 8],
        pitching_advanced: [None; 6],
    }
}

#[test]
fn ranking_preserves_rate_polarity_and_name_ties() {
    let hitter = player("Ada", "B", [0., 0., 10., 2., 8., 1., 0.300], [0.; 7]);
    let pitcher = player("Ben", "P", [0.; 7], [10., 0., 1., 2., 20., 5., 1.5]);
    assert!(evaluate(&hitter).score > 0.0);
    assert!(evaluate(&pitcher).score > 0.0);
    let mut tied = vec![
        player("Zed", "B", [0.; 7], [0.; 7]),
        player("Ada", "B", [0.; 7], [0.; 7]),
    ];
    sort_by_evaluation(&mut tied);
    assert_eq!(tied[0].name, "Ada");
}

#[test]
fn pitcher_ranking_rewards_lower_rates_only_with_recorded_innings() {
    let good_rates = player("Good", "P", [0.; 7], [10., 0., 1., 0., 10., 2.50, 1.00]);
    let bad_rates = player("Bad", "P", [0.; 7], [10., 0., 1., 0., 10., 5.00, 1.50]);
    let no_innings = player("None", "P", [0.; 7], [0., 0., 1., 0., 10., 2.50, 1.00]);

    assert!(evaluate(&good_rates).score > evaluate(&bad_rates).score);
    assert_eq!(evaluate(&no_innings).score, 16.0);
}
