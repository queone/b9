use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use b9::store::{Clock, RosterWrite, Store};
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
    assert_eq!(store.schema_version().unwrap(), 1);
}
