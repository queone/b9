use rusqlite::Connection;
use skout::store::{FangraphsBattedBallWrite, ProjectionWrite, Store};
use tempfile::tempdir;

fn projection(mlbam_id: i64, source: &str) -> ProjectionWrite {
    ProjectionWrite {
        mlbam_id,
        season: 2026,
        source: source.into(),
        stat_group: "batting".into(),
        pa: 600.0,
        hr: 20.0,
        ..Default::default()
    }
}

#[test]
fn complete_fangraphs_replacement_removes_obsolete_rows_and_failed_input_retains_last_good() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("analysis.db");
    Store::open_at(&path).unwrap().close().unwrap();
    let connection = Connection::open(&path).unwrap();
    connection.execute("INSERT INTO players(mlbam_id,name,mlb_team,eligible_positions,mlbam_match_source,is_closer,synced_at) VALUES(1,'Ace One','NYY','RP','seed',1,1),(2,'Ace Two','BOS','RP','seed',1,1)", []).unwrap();
    drop(connection);
    let mut store = Store::open_at(&path).unwrap();
    let batted = |id| FangraphsBattedBallWrite {
        mlbam_id: id,
        season: 2026,
        fb_pct: 0.4,
        hr_fb_pct: 0.2,
    };
    store
        .replace_fangraphs_snapshot(
            2026,
            &[projection(1, "steamer"), projection(2, "steamer")],
            &[batted(1), batted(2)],
            &[("NYY".into(), "Ace One".into())],
        )
        .unwrap();
    store
        .replace_fangraphs_snapshot(
            2026,
            &[projection(1, "blend")],
            &[batted(1)],
            &[("NYY".into(), "Ace One".into())],
        )
        .unwrap();
    assert!(
        store
            .replace_fangraphs_snapshot(2026, &[], &[batted(1)], &[])
            .is_err()
    );
    store.close().unwrap();
    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM player_projections", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM fangraphs_batted_ball", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row("SELECT source FROM player_projections", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "blend"
    );
}

#[test]
fn fantasypros_primary_fallback_and_ambiguous_identity_rules_are_atomic() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("ecr.db");
    Store::open_at(&path).unwrap().close().unwrap();
    let connection = Connection::open(&path).unwrap();
    connection.execute("INSERT INTO players(yahoo_player_id,name,mlb_team,synced_at) VALUES(10,'Primary','NYY',1),(11,'Fallback','BOS',1),(12,'Twin','SEA',1),(13,'Twin','SEA',1)", []).unwrap();
    drop(connection);
    let mut store = Store::open_at(&path).unwrap();
    assert_eq!(
        store
            .replace_ecr(&[
                (Some(10), "ignored".into(), "XXX".into(), 1),
                (None, "Fallback".into(), "BOS".into(), 2),
                (None, "Twin".into(), "SEA".into(), 3)
            ])
            .unwrap(),
        2
    );
    store.close().unwrap();
    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM players WHERE ecr IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        2
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT ecr FROM players WHERE yahoo_player_id=10",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}
