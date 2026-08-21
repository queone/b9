use std::collections::BTreeMap;

use skout::providers::fangraphs::{LeaderRow, ProjectionRow, resolve_mlbam_id};

#[test]
fn projection_and_leader_rows_decode_alphanumeric_ids() {
    // Real production projection responses carry FanGraphs's own
    // alphanumeric ids (e.g. "sa3020134") on the large majority of rows,
    // never a bare integer — the deserializer must accept them as-is.
    let row: ProjectionRow = serde_json::from_value(serde_json::json!({
        "playerid": "sa3020134",
        "xMLBAMID": 702518,
        "PA": 500.0
    }))
    .unwrap();
    assert_eq!(row.fangraphs_id, "sa3020134");
    assert_eq!(row.mlbam_id, Some(702518));
    assert_eq!(row.pa, 500.0);

    let leader: LeaderRow = serde_json::from_value(serde_json::json!({
        "playerid": "sa3020134",
        "xMLBAMID": 702518,
        "FB%": 40.0,
        "HR/FB": 12.5
    }))
    .unwrap();
    assert_eq!(leader.fangraphs_id, "sa3020134");
    assert_eq!(leader.mlbam_id, Some(702518));
}

#[test]
fn projection_row_without_xmlbamid_still_decodes() {
    let row: ProjectionRow = serde_json::from_value(serde_json::json!({
        "playerid": "sa3012665",
        "xMLBAMID": null,
        "PA": 1.0
    }))
    .unwrap();
    assert_eq!(row.fangraphs_id, "sa3012665");
    assert_eq!(row.mlbam_id, None);
}

#[test]
fn resolve_mlbam_id_prefers_the_row_own_id_over_the_crosswalk() {
    let mut crosswalk = BTreeMap::new();
    crosswalk.insert("sa3020134".to_string(), 111);
    assert_eq!(
        resolve_mlbam_id(Some(702518), "sa3020134", &crosswalk),
        Some(702518)
    );
}

#[test]
fn resolve_mlbam_id_falls_back_to_the_leaderboard_crosswalk() {
    let mut crosswalk = BTreeMap::new();
    crosswalk.insert("sa3020134".to_string(), 702518);
    assert_eq!(
        resolve_mlbam_id(None, "sa3020134", &crosswalk),
        Some(702518)
    );
}

#[test]
fn resolve_mlbam_id_is_none_when_unresolved_either_way() {
    let crosswalk = BTreeMap::new();
    assert_eq!(resolve_mlbam_id(None, "sa9999999", &crosswalk), None);
}
