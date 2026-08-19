use b9::store::{StatcastWrite, Store};
use rusqlite::Connection;
use tempfile::tempdir;

fn row(id: i64) -> StatcastWrite {
    StatcastWrite {
        mlbam_id: id,
        season: 2026,
        stat_group: "batting".into(),
        plate_appearances: 240,
        batted_ball_events: 160,
        xwoba: Some(0.401),
        exit_velo_avg: Some(94.2),
        barrel_pct: Some(15.3),
        hard_hit_pct: Some(52.1),
        sprint_speed: Some(28.7),
        strikeout_pct: Some(20.4),
        walk_pct: Some(11.2),
        ops: Some(0.950),
        fastball_velo: None,
        whiff_pct: None,
        chase_pct: None,
        gb_pct: None,
    }
}

#[test]
fn statcast_snapshot_is_complete_and_prefers_non_seed_identity() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("b9.db");
    let mut store = Store::open_at(&path).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection.execute("INSERT INTO players(mlbam_id,name,mlbam_match_source,synced_at) VALUES(700001,'Seed','seed',1)", []).unwrap();
    connection.execute("INSERT INTO players(yahoo_player_id,mlbam_id,name,mlbam_match_source,synced_at) VALUES(1,700001,'Yahoo','name',1)", []).unwrap();
    assert_eq!(
        store
            .replace_statcast_snapshot(2026, "batting", &[row(700001)])
            .unwrap(),
        1
    );
    let owner: String = connection
        .query_row(
            "SELECT p.name FROM statcast_seasons s JOIN players p ON p.id=s.player_id",
            [],
            |result| result.get(0),
        )
        .unwrap();
    assert_eq!(owner, "Yahoo");
    assert!(
        store
            .replace_statcast_snapshot(2026, "pitching", &[row(700001)])
            .is_err()
    );
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM statcast_seasons", [], |result| {
            result.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
}
