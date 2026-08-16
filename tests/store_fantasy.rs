use b9::domain::{FantasyPlayer, FantasyRosterSlot, FantasyTeam, League, Position, ScoringType};
use b9::store::{CategoryWrite, FantasySnapshotWrite, PositionWrite, Store};
use tempfile::tempdir;

fn snapshot() -> FantasySnapshotWrite {
    FantasySnapshotWrite {
        league: League {
            league_key: "mlb.l.1".into(),
            name: "League".into(),
            season: 2026,
            num_teams: 2,
            scoring_type: ScoringType::HeadToHead,
            roster_positions: vec![Position::Outfield],
            batting_categories: vec!["R".into()],
            pitching_categories: vec![],
        },
        current_week: Some(7),
        categories: vec![CategoryWrite {
            stat_id: 7,
            abbreviation: "R".into(),
            name: "Runs".into(),
            sort_order: 1,
            display_only: false,
            sequence: 0,
        }],
        positions: vec![PositionWrite {
            position: "OF".into(),
            count: 1,
        }],
        teams: vec![
            FantasyTeam {
                team_key: "mlb.l.1.t.1".into(),
                league_key: "mlb.l.1".into(),
                team_id: 1,
                name: "One".into(),
                manager_name: "A".into(),
                is_owned_by_current_login: true,
            },
            FantasyTeam {
                team_key: "mlb.l.1.t.2".into(),
                league_key: "mlb.l.1".into(),
                team_id: 2,
                name: "Two".into(),
                manager_name: "B".into(),
                is_owned_by_current_login: false,
            },
        ],
        players: vec![
            FantasyPlayer {
                yahoo_player_id: 101,
                name: "Ada Hitter".into(),
                mlb_team: "NYY".into(),
                display_position: "OF".into(),
                position_type: "B".into(),
                eligible_positions: vec![Position::Outfield],
                injury_status: String::new(),
                percent_owned: Some(99.0),
                yahoo_rank: Some(1),
            },
            FantasyPlayer {
                yahoo_player_id: 102,
                name: "Grace Hitter".into(),
                mlb_team: "BOS".into(),
                display_position: "OF".into(),
                position_type: "B".into(),
                eligible_positions: vec![Position::Outfield],
                injury_status: String::new(),
                percent_owned: Some(98.0),
                yahoo_rank: Some(2),
            },
        ],
        slots: vec![
            FantasyRosterSlot {
                team_key: "mlb.l.1.t.1".into(),
                yahoo_player_id: 101,
                slot_position: Position::Outfield,
            },
            FantasyRosterSlot {
                team_key: "mlb.l.1.t.2".into(),
                yahoo_player_id: 102,
                slot_position: Position::Outfield,
            },
        ],
    }
}

#[test]
fn complete_snapshot_replaces_scoped_rows_on_schema_one() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
    store.replace_fantasy_snapshot(&snapshot()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 1);
    assert_eq!(store.fantasy_current_week("mlb.l.1").unwrap(), Some(7));
    assert_eq!(store.fantasy_teams("mlb.l.1").unwrap().len(), 2);
    let mut invalid = snapshot();
    invalid.slots[0].team_key = "other".into();
    assert!(store.replace_fantasy_snapshot(&invalid).is_err());
    assert_eq!(store.fantasy_teams("mlb.l.1").unwrap().len(), 2);
}
