use b9::store::{LegacyBootstrap, Store};

#[test]
fn guarded_bootstrap_maps_legacy_identity_ownership_stats_and_freshness() {
    let temporary = tempfile::tempdir().unwrap();
    let source_path = temporary.path().join("skout.db");
    let target_path = temporary.path().join("b9.db");
    let mut source = Store::open_at(&source_path).unwrap();
    source
        .transaction(|tx| {
            tx.execute("INSERT INTO yahoo_leagues(league_key,name,season,num_teams,scoring_type,synced_at) VALUES('l.1','League',2026,12,'head',100)", []).unwrap();
            tx.execute("INSERT INTO yahoo_teams(team_key,league_key,team_id,name,synced_at) VALUES('l.1.t.1','l.1',1,'Owner Team',100)", []).unwrap();
            tx.execute("INSERT INTO players(mlbam_id,yahoo_player_id,name,mlb_team,display_position,position_type,eligible_positions,status,is_closer,yahoo_rank,bat_side,mlbam_match_source,synced_at) VALUES(7,70,'Yahoo Name','NYY','2B','B','2B,3B','IL10',0,87,'L','name+team',100)", []).unwrap();
            let yahoo_id = tx.last_insert_rowid();
            tx.execute("INSERT INTO players(mlbam_id,name,mlb_team,display_position,position_type,mlbam_match_source,synced_at) VALUES(7,'Seed Name','NYY','2B','B','seed',100)", []).unwrap();
            let seed_id = tx.last_insert_rowid();
            tx.execute("INSERT INTO yahoo_roster_slots(team_key,player_id,slot_position,synced_at) VALUES('l.1.t.1',?1,'2B',100)", [yahoo_id]).unwrap();
            tx.execute("INSERT INTO mlbam_season_stats(player_id,season,stat_group,pa,obp,r,hr,rbi,sb,avg,synced_at) VALUES(?1,2026,'hitting',447,.295,56,17,48,31,.216,100)", [seed_id]).unwrap();
            tx.execute("INSERT INTO sync_log(table_name,synced_at) VALUES('rosters',100)", []).unwrap();
            Ok(())
        })
        .unwrap();
    source.close().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&source_path, std::fs::Permissions::from_mode(0o444)).unwrap();
    }

    let mut target = Store::open_at(&target_path).unwrap();
    assert_eq!(
        target.bootstrap_legacy_at(&source_path).unwrap(),
        LegacyBootstrap::Imported
    );
    target
        .transaction(|tx| {
            tx.execute("INSERT INTO mlb_team_active_rosters(team_abbr,mlbam_id,primary_type,status,fetched_at) VALUES('NYY',7,'H','A',100)", []).unwrap();
            Ok(())
        })
        .unwrap();
    let rows = target.mlb_roster("NYY").unwrap();
    assert_eq!(rows[0].name, "Yahoo Name");
    assert_eq!(rows[0].eligible_positions, "2B,3B");
    assert_eq!(rows[0].injury_status, "IL10");
    assert_eq!(rows[0].yahoo_rank, Some(87));
    assert_eq!(rows[0].owner.as_deref(), Some("Owner Team"));
    assert_eq!(rows[0].plate_appearances, 447);
    assert_eq!(target.ownership_synced_at().unwrap(), Some(100));
    assert_eq!(
        target.bootstrap_legacy_at(&source_path).unwrap(),
        LegacyBootstrap::AlreadySatisfied
    );
}

#[test]
fn absent_source_is_nonfatal_and_retryable() {
    let temporary = tempfile::tempdir().unwrap();
    let mut target = Store::open_at(temporary.path().join("b9.db")).unwrap();
    assert_eq!(
        target
            .bootstrap_legacy_at(&temporary.path().join("missing.db"))
            .unwrap(),
        LegacyBootstrap::SourceAbsent
    );
}

#[test]
fn incompatible_source_rolls_back_and_remains_retryable() {
    let temporary = tempfile::tempdir().unwrap();
    let source_path = temporary.path().join("incomplete.db");
    let target_path = temporary.path().join("b9.db");
    let mut source = Store::open_at(&source_path).unwrap();
    source
        .transaction(|tx| {
            tx.execute("INSERT INTO yahoo_leagues(league_key,name,season,num_teams,scoring_type,synced_at) VALUES('l.2','Broken',2026,12,'head',100)", []).unwrap();
            tx.execute("DROP TABLE mlbam_season_stats", []).unwrap();
            Ok(())
        })
        .unwrap();
    source.close().unwrap();
    let mut target = Store::open_at(&target_path).unwrap();
    assert!(target.bootstrap_legacy_at(&source_path).is_err());
    assert!(target.is_empty().unwrap());
    assert!(target.bootstrap_legacy_at(&source_path).is_err());
}
