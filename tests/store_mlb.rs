use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use b9::store::{Clock, RosterWrite, SeasonStatWrite, Store};
use rusqlite::Connection;
use tempfile::tempdir;

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(10)
    }
}

#[test]
fn roster_replacement_preserves_two_way_roles_and_rejects_empty_overwrite() {
    let dir = tempdir().unwrap();
    let mut store =
        Store::open_at_with_clock(dir.path().join("b9.db"), Arc::new(FixedClock)).unwrap();
    let rows = vec![
        RosterWrite {
            mlbam_id: 17,
            name: "Two Way".into(),
            position: "TWP".into(),
            primary_type: "H".into(),
            status: "A".into(),
            jersey_number: "17".into(),
        },
        RosterWrite {
            mlbam_id: 17,
            name: "Two Way".into(),
            position: "TWP".into(),
            primary_type: "P".into(),
            status: "A".into(),
            jersey_number: "17".into(),
        },
    ];
    store.replace_mlb_roster("LAA", &rows).unwrap();
    assert_eq!(store.mlb_roster("LAA").unwrap().len(), 2);
    let connection = Connection::open(dir.path().join("b9.db")).unwrap();
    connection.execute("INSERT INTO players (mlbam_id,name,mlb_team,position_type,synced_at) VALUES (17,'Duplicate','LAA','H',10)", []).unwrap();
    assert_eq!(store.mlb_roster("LAA").unwrap().len(), 2);
    assert!(store.replace_mlb_roster("LAA", &[]).is_err());
    assert_eq!(
        store.schema_version().unwrap(),
        b9::store::CURRENT_SCHEMA_VERSION
    );
}

#[test]
fn bulk_stats_preserve_separately_acquired_quality_starts() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("b9.db");
    let mut store = Store::open_at_with_clock(&path, Arc::new(FixedClock)).unwrap();
    let mut pitching = SeasonStatWrite {
        mlbam_id: 700003,
        name: "QS Pitcher".into(),
        team_abbreviation: "NYY".into(),
        stat_group: "pitching".into(),
        wins: 5,
        strikeouts: 60,
        innings_outs: 20,
        quality_starts: 5,
        ..SeasonStatWrite::default()
    };
    store
        .replace_mlb_season_stats(2026, &[pitching.clone()])
        .unwrap();
    pitching.wins = 6;
    pitching.strikeouts = 70;
    pitching.quality_starts = 0;
    store.replace_mlb_season_stats(2026, &[pitching]).unwrap();

    let connection = Connection::open(path).unwrap();
    let values = connection
        .query_row(
            "SELECT qs,w,k,ip FROM mlbam_season_stats WHERE season=2026 AND stat_group='pitching'",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, f64>(3)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(values.0, 5);
    assert_eq!(values.1, 6);
    assert_eq!(values.2, 70);
    assert!((values.3 - 6.2).abs() < f64::EPSILON);
}
