//! Guarded, one-time import from the legacy local compatibility store.

use std::path::{Path, PathBuf};

use rusqlite::{OpenFlags, OptionalExtension, params};

use super::{Store, StoreError};

const MARKER: &str = "legacy_skout_bootstrap";

/// Outcome of attempting the guarded legacy bootstrap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyBootstrap {
    Imported,
    AlreadySatisfied,
    SourceAbsent,
}

impl Store {
    /// Import compatible fantasy context when b9 has none, preserving newer b9 rows.
    pub fn bootstrap_legacy_at(&mut self, source: &Path) -> Result<LegacyBootstrap, StoreError> {
        const OP: &str = "bootstrap legacy local data";
        let satisfied = self
            .connection()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM players WHERE yahoo_player_id IS NOT NULL) OR EXISTS(SELECT 1 FROM sync_log WHERE table_name=?1)",
                [MARKER],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| StoreError::operation(OP, &self.path, error))?;
        if satisfied {
            return Ok(LegacyBootstrap::AlreadySatisfied);
        }
        if !source.is_file() {
            return Ok(LegacyBootstrap::SourceAbsent);
        }

        // Prove the compatibility source can be opened read-only before attaching it.
        let source_connection = rusqlite::Connection::open_with_flags(
            source,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(|error| StoreError::operation(OP, source, error))?;
        source_connection
            .query_row("SELECT version FROM schema_version", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|error| StoreError::operation(OP, source, error))?;
        drop(source_connection);

        let path = self.path.clone();
        if !source.is_absolute() {
            return Err(StoreError::invalid(
                OP,
                "legacy database path is not absolute",
            ));
        }
        let source_uri = format!("file:{}?mode=ro", source.display());
        self.connection_mut()
            .execute("ATTACH DATABASE ?1 AS legacy", [source_uri])
            .map_err(|error| StoreError::operation(OP, &path, error))?;
        let (_, now) = self.captured_time(OP)?;
        let result = self.transaction(|tx| {
            tx.execute_batch(
                "INSERT OR IGNORE INTO yahoo_leagues SELECT * FROM legacy.yahoo_leagues;
                 INSERT OR IGNORE INTO yahoo_stat_categories SELECT * FROM legacy.yahoo_stat_categories;
                 INSERT OR IGNORE INTO yahoo_roster_positions SELECT * FROM legacy.yahoo_roster_positions;
                 INSERT OR IGNORE INTO yahoo_teams SELECT * FROM legacy.yahoo_teams;

                 INSERT INTO players (mlbam_id,yahoo_player_id,name,mlb_team,display_position,position_type,eligible_positions,status,percent_owned,ownership_delta,is_undroppable,is_closer,yahoo_rank,bat_side,pitch_hand,pct_started,injury_note,injury_note_ts,mlbam_injury_note,pqs,fangraphs_war,wrc_plus,ecr,mlbam_match_source,mlbam_matched_at,birth_date,birth_date_fetched_at,jersey_number,synced_at)
                 SELECT mlbam_id,yahoo_player_id,name,mlb_team,display_position,position_type,eligible_positions,status,percent_owned,ownership_delta,is_undroppable,is_closer,yahoo_rank,bat_side,pitch_hand,pct_started,injury_note,injury_note_ts,mlbam_injury_note,pqs,fangraphs_war,wrc_plus,ecr,mlbam_match_source,mlbam_matched_at,birth_date,birth_date_fetched_at,jersey_number,synced_at
                 FROM legacy.players lp WHERE lp.yahoo_player_id IS NOT NULL
                 ON CONFLICT(yahoo_player_id) DO UPDATE SET
                   mlbam_id=excluded.mlbam_id,name=excluded.name,mlb_team=excluded.mlb_team,display_position=excluded.display_position,position_type=excluded.position_type,eligible_positions=excluded.eligible_positions,status=excluded.status,percent_owned=excluded.percent_owned,ownership_delta=excluded.ownership_delta,is_undroppable=excluded.is_undroppable,is_closer=excluded.is_closer,yahoo_rank=excluded.yahoo_rank,bat_side=excluded.bat_side,pitch_hand=excluded.pitch_hand,pct_started=excluded.pct_started,injury_note=excluded.injury_note,injury_note_ts=excluded.injury_note_ts,mlbam_injury_note=excluded.mlbam_injury_note,pqs=excluded.pqs,fangraphs_war=excluded.fangraphs_war,wrc_plus=excluded.wrc_plus,ecr=excluded.ecr,mlbam_match_source=excluded.mlbam_match_source,mlbam_matched_at=excluded.mlbam_matched_at,birth_date=excluded.birth_date,birth_date_fetched_at=excluded.birth_date_fetched_at,jersey_number=excluded.jersey_number,synced_at=excluded.synced_at
                 WHERE players.synced_at < excluded.synced_at;

                 INSERT INTO players (mlbam_id,name,mlb_team,display_position,position_type,eligible_positions,status,percent_owned,ownership_delta,is_undroppable,is_closer,yahoo_rank,bat_side,pitch_hand,pct_started,injury_note,injury_note_ts,mlbam_injury_note,pqs,fangraphs_war,wrc_plus,ecr,mlbam_match_source,mlbam_matched_at,birth_date,birth_date_fetched_at,jersey_number,synced_at)
                 SELECT lp.mlbam_id,lp.name,lp.mlb_team,lp.display_position,lp.position_type,lp.eligible_positions,lp.status,lp.percent_owned,lp.ownership_delta,lp.is_undroppable,lp.is_closer,lp.yahoo_rank,lp.bat_side,lp.pitch_hand,lp.pct_started,lp.injury_note,lp.injury_note_ts,lp.mlbam_injury_note,lp.pqs,lp.fangraphs_war,lp.wrc_plus,lp.ecr,lp.mlbam_match_source,lp.mlbam_matched_at,lp.birth_date,lp.birth_date_fetched_at,lp.jersey_number,lp.synced_at
                 FROM legacy.players lp WHERE lp.yahoo_player_id IS NULL AND lp.mlbam_id IS NOT NULL
                   AND NOT EXISTS (SELECT 1 FROM players p WHERE p.mlbam_id=lp.mlbam_id AND COALESCE(p.position_type,'')=COALESCE(lp.position_type,'') AND COALESCE(p.mlbam_match_source,'')=COALESCE(lp.mlbam_match_source,''));

                 INSERT OR REPLACE INTO yahoo_roster_slots(team_key,player_id,slot_position,synced_at)
                 SELECT lrs.team_key,dp.id,lrs.slot_position,lrs.synced_at
                 FROM legacy.yahoo_roster_slots lrs JOIN legacy.players lp ON lp.id=lrs.player_id
                 JOIN players dp ON dp.yahoo_player_id=lp.yahoo_player_id;

                 INSERT OR IGNORE INTO mlbam_season_stats(player_id,season,stat_group,g,pa,ab,h,hr,rbi,r,sb,avg,obp,so_bat,doubles,triples,cs,bb,hbp,tb,slg,ops,sf,sh,gidp,ibb,babip,w,l,sv,hld,k,era,whip,ip,qs,gs,h_pit,r_pit,er,hr_pit,bb_pit,hbp_pit,bk,wp,bf,gf,svo,bs,cg,sho,ibb_pit,k9,bb9,h9,hr9,kbb,inherited_runners,inherited_runners_scored,pickoffs,sb_allowed,cs_allowed,pitches,pitches_per_inn,fip,fangraphs_war,wrc_plus,synced_at)
                 SELECT dp.id,s.season,s.stat_group,s.g,s.pa,s.ab,s.h,s.hr,s.rbi,s.r,s.sb,s.avg,s.obp,s.so_bat,s.doubles,s.triples,s.cs,s.bb,s.hbp,s.tb,s.slg,s.ops,s.sf,s.sh,s.gidp,s.ibb,s.babip,s.w,s.l,s.sv,s.hld,s.k,s.era,s.whip,s.ip,s.qs,s.gs,s.h_pit,s.r_pit,s.er,s.hr_pit,s.bb_pit,s.hbp_pit,s.bk,s.wp,s.bf,s.gf,s.svo,s.bs,s.cg,s.sho,s.ibb_pit,s.k9,s.bb9,s.h9,s.hr9,s.kbb,s.inherited_runners,s.inherited_runners_scored,s.pickoffs,s.sb_allowed,s.cs_allowed,s.pitches,s.pitches_per_inn,s.fip,s.fangraphs_war,s.wrc_plus,s.synced_at
                 FROM legacy.mlbam_season_stats s JOIN legacy.players lp ON lp.id=s.player_id
                 JOIN players dp ON dp.id=(SELECT p.id FROM players p WHERE (lp.yahoo_player_id IS NOT NULL AND p.yahoo_player_id=lp.yahoo_player_id) OR (lp.yahoo_player_id IS NULL AND p.mlbam_id=lp.mlbam_id AND COALESCE(p.position_type,'')=COALESCE(lp.position_type,'')) ORDER BY CASE WHEN COALESCE(p.mlbam_match_source,'')='seed' THEN 0 ELSE 1 END,p.id LIMIT 1);

                 INSERT OR REPLACE INTO sync_log(table_name,synced_at)
                 SELECT table_name,synced_at FROM legacy.sync_log WHERE table_name='rosters';",
            )
            .map_err(|error| StoreError::operation(OP, &path, error))?;
            tx.execute(
                "INSERT OR REPLACE INTO sync_log(table_name,synced_at) VALUES (?1,?2)",
                params![MARKER, now],
            )
            .map_err(|error| StoreError::operation(OP, &path, error))?;
            Ok(())
        });
        let detach = self
            .connection_mut()
            .execute("DETACH DATABASE legacy", [])
            .map_err(|error| StoreError::operation(OP, &path, error));
        result?;
        detach?;
        Ok(LegacyBootstrap::Imported)
    }

    /// Import from the established legacy user path when it exists.
    pub fn bootstrap_legacy(&mut self) -> Result<LegacyBootstrap, StoreError> {
        self.bootstrap_legacy_at(&legacy_database_path()?)
    }

    /// Read the durable Yahoo ownership freshness timestamp.
    pub fn ownership_synced_at(&self) -> Result<Option<i64>, StoreError> {
        self.connection()
            .query_row(
                "SELECT synced_at FROM sync_log WHERE table_name='rosters'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| StoreError::operation("read ownership freshness", &self.path, error))
    }
}

fn legacy_database_path() -> Result<PathBuf, StoreError> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config").join("skout").join("skout.db"))
        .ok_or(StoreError::HomeUnavailable)
}
