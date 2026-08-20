use b9::domain::{FantasyPlayer, FantasyRosterSlot, FantasyTeam, League, Position, ScoringType};
use b9::store::{
    CURRENT_SCHEMA_VERSION, CategoryWrite, FantasySnapshotWrite, PositionWrite, Store,
};
use rusqlite::Connection;
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
                waiver_priority: 1,
                faab_balance: 65,
                wins: 10,
                losses: 4,
                ties: 1,
                moves: 12,
                rank: 1,
            },
            FantasyTeam {
                team_key: "mlb.l.1.t.2".into(),
                league_key: "mlb.l.1".into(),
                team_id: 2,
                name: "💎 Two".into(),
                manager_name: "B".into(),
                is_owned_by_current_login: false,
                waiver_priority: 2,
                faab_balance: 50,
                wins: 8,
                losses: 6,
                ties: 1,
                moves: 10,
                rank: 2,
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
                percentage_started: Some(80.0),
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
                percentage_started: Some(70.0),
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
    assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert_eq!(store.fantasy_current_week("mlb.l.1").unwrap(), Some(7));
    assert_eq!(store.fantasy_season("mlb.l.1").unwrap(), Some(2026));
    assert_eq!(
        store.fantasy_categories("mlb.l.1").unwrap()[0].abbreviation,
        "R"
    );
    let teams = store.fantasy_teams("mlb.l.1").unwrap();
    assert_eq!(teams.len(), 2);
    assert_eq!(teams[0].manager_name, "A");
    assert_eq!(teams[1].name, "Two");
    assert_eq!(
        store
            .fantasy_players("mlb.l.1")
            .unwrap()
            .into_iter()
            .find(|player| player.yahoo_player_id == Some(102))
            .unwrap()
            .owner
            .as_deref(),
        Some("Two")
    );
    assert_eq!(
        store
            .fantasy_players("mlb.l.1")
            .unwrap()
            .into_iter()
            .find(|player| player.yahoo_player_id == Some(102))
            .unwrap()
            .percentage_started,
        70.0
    );
    let mut invalid = snapshot();
    invalid.slots[0].team_key = "other".into();
    assert!(store.replace_fantasy_snapshot(&invalid).is_err());
    assert_eq!(store.fantasy_teams("mlb.l.1").unwrap().len(), 2);
}

#[test]
fn yahoo_mlb_identity_lookup_includes_players_outside_league_views() {
    let directory = tempdir().unwrap();
    let store = Store::open_at(directory.path().join("b9.db")).unwrap();
    Connection::open(store.path())
        .unwrap()
        .execute(
            "INSERT INTO players(yahoo_player_id,mlbam_id,name,synced_at) VALUES(999,700999,'Matchup Only',1)",
            [],
        )
        .unwrap();

    assert_eq!(
        store
            .mlb_identities_for_yahoo_players(&[999, 1000])
            .unwrap(),
        vec![(999, 700999)]
    );
    assert_eq!(
        store.yahoo_player_metadata(&[999, 1000]).unwrap(),
        vec![(999, "Matchup Only".into(), String::new(), String::new())]
    );
    assert!(store.fantasy_players("mlb.l.1").unwrap().is_empty());
}

#[test]
fn complete_free_agent_replacement_is_scoped_to_its_league() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
    let mut first = snapshot();
    first.players.push(FantasyPlayer {
        yahoo_player_id: 103,
        name: "Charlie FreeAgent".into(),
        mlb_team: "TB".into(),
        display_position: "OF".into(),
        position_type: "B".into(),
        eligible_positions: vec![Position::Outfield],
        injury_status: String::new(),
        percent_owned: Some(10.0),
        percentage_started: Some(20.0),
        yahoo_rank: Some(3),
    });
    store.replace_fantasy_snapshot(&first).unwrap();
    assert_eq!(store.fantasy_players("mlb.l.1").unwrap().len(), 3);
    let mut second = first.clone();
    second.league.league_key = "mlb.l.2".into();
    for team in &mut second.teams {
        team.league_key = "mlb.l.2".into();
        team.team_key = team.team_key.replacen("mlb.l.1", "mlb.l.2", 1);
    }
    for slot in &mut second.slots {
        slot.team_key = slot.team_key.replacen("mlb.l.1", "mlb.l.2", 1);
    }
    store.replace_fantasy_snapshot(&second).unwrap();
    assert_eq!(store.fantasy_players("mlb.l.2").unwrap().len(), 3);

    let mut replacement = first;
    replacement.players.pop();
    store.replace_fantasy_snapshot(&replacement).unwrap();
    assert_eq!(store.fantasy_players("mlb.l.1").unwrap().len(), 2);
    assert_eq!(store.fantasy_players("mlb.l.2").unwrap().len(), 3);
}

#[test]
fn fantasy_players_ignore_legacy_unassigned_roster_slots() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("b9.db");
    let mut store = Store::open_at(&path).unwrap();
    store.replace_fantasy_snapshot(&snapshot()).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE yahoo_roster_slots SET slot_position='--' WHERE player_id=(SELECT id FROM players WHERE yahoo_player_id=101)",
            [],
        )
        .unwrap();

    let players = store.fantasy_players("mlb.l.1").unwrap();

    assert!(
        players
            .iter()
            .all(|player| player.yahoo_player_id != Some(101))
    );
}

#[test]
fn fantasy_players_join_stats_through_mlbam_identity_not_duplicate_row_choice() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("b9.db");
    let mut store = Store::open_at(&path).unwrap();
    store.replace_fantasy_snapshot(&snapshot()).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE players SET mlbam_id=700001,mlbam_match_source='name',injury_note='Right rib stress fracture',birth_date='1992-04-26' WHERE yahoo_player_id=101",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO players (mlbam_id,name,position_type,mlbam_match_source,synced_at) VALUES (700001,'Ada Hitter','H','seed',20)",
            [],
        )
        .unwrap();
    let stats_player = connection.last_insert_rowid();
    connection
        .execute(
            "INSERT INTO mlbam_season_stats (player_id,season,stat_group,pa,r,avg,synced_at) VALUES (?1,2026,'hitting',500,75,.250,20)",
            [stats_player],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO mlbam_season_stats (player_id,season,stat_group,pa,r,avg,synced_at) VALUES (?1,2025,'hitting',400,60,.240,10)",
            [stats_player],
        )
        .unwrap();
    let yahoo_player: i64 = connection
        .query_row(
            "SELECT id FROM players WHERE yahoo_player_id=101",
            [],
            |row| row.get(0),
        )
        .unwrap();
    connection.execute(
        "INSERT INTO mlbam_season_stats (player_id,season,stat_group,pa,r,avg,synced_at) VALUES (?1,2026,'hitting',450,70,.260,5)",
        [yahoo_player],
    ).unwrap();

    let ada = store
        .fantasy_players("mlb.l.1")
        .unwrap()
        .into_iter()
        .find(|player| player.yahoo_player_id == Some(101))
        .unwrap();
    assert_eq!(ada.batting[0], 450.0);
    assert_eq!(ada.batting[2], 70.0);
    assert_eq!(ada.injury_note, "Right rib stress fracture");
    assert_eq!(ada.birth_date, "1992-04-26");
}
