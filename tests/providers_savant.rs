use b9::providers::savant::parse_csv;

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
            batting[0].xwoba,
            batting[0].strikeout_pct,
            batting[0].walk_pct,
            batting[0].ops
        ),
        (700001, Some(0.401), Some(20.4), Some(11.2), Some(0.950))
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
}
