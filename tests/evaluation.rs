use skout::analysis::{blend, pqs, statcast_blend, window_proj, wire_threshold};
use skout::domain::{GameIndicator, StoredFantasyPlayer};

fn player(name: &str, started: f64) -> StoredFantasyPlayer {
    StoredFantasyPlayer {
        yahoo_player_id: Some(1),
        mlbam_id: Some(1),
        name: name.into(),
        team: "NYY".into(),
        role: "B".into(),
        positions: "OF".into(),
        is_closer: false,
        status: String::new(),
        injury_note: String::new(),
        birth_date: String::new(),
        game_status: String::new(),
        game_indicator: GameIndicator::None,
        hand: "R".into(),
        rank: None,
        percent_owned: None,
        percentage_started: started,
        expert_consensus_rank: None,
        owner: None,
        slot: None,
        batting: [1.0; 7],
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
fn pqs_context_and_tie_break_are_deterministic() {
    let mut low = player("Zulu", 40.0);
    low.hitting_advanced[0] = Some(0.300);
    low.statcast_samples[0] = 25.0;
    let mut high = player("Alpha", 80.0);
    high.hitting_advanced[0] = Some(0.450);
    high.statcast_samples[0] = 500.0;
    let mut rows = vec![low, high];
    pqs::sort_by_pqs(&mut rows);
    assert_eq!(rows[0].name, "Alpha");
}

#[test]
fn blend_and_shrink_follow_contract() {
    assert_eq!(blend::weights(10, true), (0.25, 0.75));
    assert!((statcast_blend::shrink(10.0, 10.0, 0.0, 10.0) - 5.0).abs() < 1e-9);
}

#[test]
fn percentile_and_windows_cover_fallbacks() {
    assert_eq!(
        wire_threshold::percentile(vec![1.0, 2.0, 3.0], 0.5),
        Some(2.0)
    );
    assert_eq!(window_proj::blend(Some(10.0), Some(0.0)), 7.0);
    assert_eq!(window_proj::blend(Some(10.0), None), 10.0);
    assert_eq!(window_proj::blend(None, None), 0.0);
    let projected = window_proj::PitcherWindow {
        ip: 100.0,
        qs: 0.0,
        w: 10.0,
        ..Default::default()
    };
    let recent = window_proj::PitcherWindow {
        ip: 10.0,
        qs: 2.0,
        w: 2.0,
        ..Default::default()
    };
    let next = window_proj::next_pitcher(Some(projected), Some(recent), 10.0);
    assert!((next.w - 1.3).abs() < 1e-9);
    assert_eq!(next.qs, 2.0);
    assert_eq!(
        window_proj::next_pitcher(None, None, 10.0),
        Default::default()
    );
}
