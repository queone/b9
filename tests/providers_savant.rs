use skout::providers::savant::parse_csv;

#[test]
fn savant_fixtures_parse_contracted_metrics() {
    let batting = parse_csv(
        include_bytes!("fixtures/savant/batting.csv"),
        2026,
        "batting",
    )
    .unwrap();
    assert_eq!(
        (
            batting[0].mlbam_id,
            batting[0].plate_appearances,
            batting[0].batted_ball_events,
            batting[0].xwoba,
            batting[0].strikeout_pct,
            batting[0].walk_pct,
            batting[0].ops
        ),
        (
            700001,
            240,
            160,
            Some(0.401),
            Some(20.4),
            Some(11.2),
            Some(0.950)
        )
    );
    let pitching = parse_csv(
        include_bytes!("fixtures/savant/pitching.csv"),
        2026,
        "pitching",
    )
    .unwrap();
    assert_eq!(
        (
            pitching[0].fastball_velo,
            pitching[0].whiff_pct,
            pitching[0].gb_pct,
            pitching[0].strikeout_pct,
            pitching[0].walk_pct
        ),
        (Some(96.4), Some(31.2), Some(45.6), Some(30.1), Some(7.4))
    );
}

#[test]
fn savant_parser_rejects_partial_rows() {
    assert!(parse_csv(b"player_id,xwoba\n1\n", 2026, "batting").is_err());
    assert!(parse_csv(b"player_id,xwoba\n1,.400\n", 2026, "batting").is_err());
    assert!(parse_csv(b"player_id\n1\n", 2026, "fielding").is_err());
    let missing_denominator = b"player_id,pa,bbe,xwoba,exit_velocity_avg,barrel_batted_rate,hard_hit_percent,k_percent,bb_percent,sprint_speed,on_base_plus_slg\n1,,,0.400,,,,,,,\n";
    let error = parse_csv(missing_denominator, 2026, "batting")
        .unwrap_err()
        .to_string();
    assert!(error.contains("PA/BF denominator"));
    assert!(!error.contains("BBE"));
}

#[test]
fn savant_parser_degrades_bbe_dependent_metrics_instead_of_rejecting_the_row() {
    // Real production leaderboard responses currently leave `bbe` blank on
    // every row while still populating PA and the BBE-dependent metrics
    // themselves (confirmed against a captured live response) — the row
    // must be retained with those metrics unavailable, not dropped whole.
    let blank_bbe = b"player_id,pa,bbe,xwoba,exit_velocity_avg,barrel_batted_rate,hard_hit_percent,k_percent,bb_percent,sprint_speed,on_base_plus_slg\n700001,240,,.401,90.2,10.5,45.3,20.4,11.2,28.1,.950\n";
    let rows = parse_csv(blank_bbe, 2026, "batting").unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.plate_appearances, 240);
    assert_eq!(row.batted_ball_events, 0);
    assert_eq!(row.xwoba, Some(0.401));
    assert_eq!(row.exit_velo_avg, None);
    assert_eq!(row.barrel_pct, None);
    assert_eq!(row.hard_hit_pct, None);
}
