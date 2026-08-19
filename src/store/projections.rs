use rusqlite::params;

use super::{FangraphsBattedBallWrite, Store, StoreError};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectionWrite {
    pub mlbam_id: i64,
    pub season: i64,
    pub source: String,
    pub stat_group: String,
    pub pa: f64,
    pub ip: f64,
    pub hr: f64,
    pub r: f64,
    pub rbi: f64,
    pub sb: f64,
    pub avg: f64,
    pub obp: f64,
    pub slg: f64,
    pub era: f64,
    pub whip: f64,
    pub k: f64,
    pub w: f64,
    pub sv: f64,
    pub bb: f64,
}

pub type ProjectionRow = ProjectionWrite;

impl Store {
    /// Read the blended projection for one canonical player and season.
    pub fn blended_projection(
        &self,
        mlbam_id: i64,
        season: i64,
        stat_group: &str,
    ) -> Result<Option<ProjectionRow>, StoreError> {
        const OP: &str = "read blended projection";
        self.connection().query_row(
            "SELECT ?1,season,source,stat_group,pa,ip,hr,r,rbi,sb,avg,obp,slg,era,whip,k,w,sv,bb FROM player_projections WHERE player_id=(SELECT id FROM players WHERE mlbam_id=?1 ORDER BY CASE WHEN mlbam_match_source='seed' THEN 0 ELSE 1 END DESC,id LIMIT 1) AND season=?2 AND source='blend' AND stat_group=?3",
            params![mlbam_id,season,stat_group], |row| Ok(ProjectionWrite { mlbam_id:row.get(0)?,season:row.get(1)?,source:row.get(2)?,stat_group:row.get(3)?,pa:row.get(4)?,ip:row.get(5)?,hr:row.get(6)?,r:row.get(7)?,rbi:row.get(8)?,sb:row.get(9)?,avg:row.get(10)?,obp:row.get(11)?,slg:row.get(12)?,era:row.get(13)?,whip:row.get(14)?,k:row.get(15)?,w:row.get(16)?,sv:row.get(17)?,bb:row.get(18)? })
        ).optional().map_err(|e|StoreError::operation(OP,&self.path,e))
    }
    /// Atomically replace every FanGraphs-owned season dataset.
    pub fn replace_fangraphs_snapshot(
        &mut self,
        season: i64,
        projections: &[ProjectionWrite],
        batted_ball: &[FangraphsBattedBallWrite],
        closers: &[(String, String)],
    ) -> Result<usize, StoreError> {
        const OP: &str = "replace FanGraphs snapshot";
        if season <= 0 || projections.is_empty() || batted_ball.is_empty() {
            return Err(StoreError::invalid(
                OP,
                "complete season datasets are required",
            ));
        }
        let (_, now) = self.captured_time(OP)?;
        let path = self.path.clone();
        self.transaction(|tx| {
            let resolved_projection_count = projections.iter().filter_map(|row| tx.query_row("SELECT 1 FROM players WHERE mlbam_id=?1 LIMIT 1",[row.mlbam_id],|r|r.get::<_,i64>(0)).optional().ok().flatten()).count();
            let resolved_batted_count = batted_ball.iter().filter_map(|row| tx.query_row("SELECT 1 FROM players WHERE mlbam_id=?1 LIMIT 1",[row.mlbam_id],|r|r.get::<_,i64>(0)).optional().ok().flatten()).count();
            if resolved_projection_count == 0 || resolved_batted_count == 0 { return Err(StoreError::invalid(OP,"snapshot has no resolvable canonical identities")); }
            tx.execute("DELETE FROM player_projections WHERE season=?1", [season]).map_err(|e|StoreError::operation(OP,&path,e))?;
            tx.execute("DELETE FROM fangraphs_batted_ball WHERE season=?1", [season]).map_err(|e|StoreError::operation(OP,&path,e))?;
            let mut written=0;
            for row in projections {
                let id=tx.query_row("SELECT id FROM players WHERE mlbam_id=?1 ORDER BY CASE WHEN mlbam_match_source='seed' THEN 0 ELSE 1 END DESC,id LIMIT 1",[row.mlbam_id],|r|r.get::<_,i64>(0)).optional().map_err(|e|StoreError::operation(OP,&path,e))?;
                let Some(id)=id else {continue};
                tx.execute("INSERT INTO player_projections VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",params![id,season,row.source,row.stat_group,row.pa,row.ip,row.hr,row.r,row.rbi,row.sb,row.avg,row.obp,row.slg,row.era,row.whip,row.k,row.w,row.sv,row.bb,now]).map_err(|e|StoreError::operation(OP,&path,e))?;
                written+=1;
            }
            for row in batted_ball {
                let id=tx.query_row("SELECT id FROM players WHERE mlbam_id=?1 ORDER BY CASE WHEN mlbam_match_source='seed' THEN 0 ELSE 1 END DESC,id LIMIT 1",[row.mlbam_id],|r|r.get::<_,i64>(0)).optional().map_err(|e|StoreError::operation(OP,&path,e))?;
                let Some(id)=id else {continue};
                tx.execute("INSERT INTO fangraphs_batted_ball VALUES(?1,?2,?3,?4,?5)",params![id,season,row.fb_pct,row.hr_fb_pct,now]).map_err(|e|StoreError::operation(OP,&path,e))?;
            }
            tx.execute("UPDATE players SET is_closer=0 WHERE eligible_positions LIKE '%RP%'",[]).map_err(|e|StoreError::operation(OP,&path,e))?;
            for(team,name) in closers { tx.execute("UPDATE players SET is_closer=1 WHERE id=(SELECT id FROM players WHERE mlb_team=?1 AND LOWER(name)=LOWER(?2) AND eligible_positions LIKE '%RP%' GROUP BY LOWER(name),mlb_team HAVING COUNT(*)=1)",params![team,name]).map_err(|e|StoreError::operation(OP,&path,e))?; }
            tx.execute("UPDATE players SET is_closer=1 WHERE id IN (SELECT p.id FROM players p JOIN mlbam_season_stats s ON s.player_id=p.id AND s.stat_group='pitching' AND s.season=?1 WHERE p.eligible_positions LIKE '%RP%' AND NOT EXISTS(SELECT 1 FROM players c WHERE c.mlb_team=p.mlb_team AND c.is_closer=1) AND s.sv=(SELECT MAX(s2.sv) FROM mlbam_season_stats s2 JOIN players p2 ON p2.id=s2.player_id WHERE p2.mlb_team=p.mlb_team AND s2.stat_group='pitching' AND s2.season=?1))",[season]).map_err(|e|StoreError::operation(OP,&path,e))?;
            Ok(written)
        })
    }
    /// Replace closer designations by resolved team/name rows with an SV fallback.
    pub fn replace_closers(&mut self, rows: &[(String, String)]) -> Result<usize, StoreError> {
        const OP: &str = "replace closer designations";
        let path = self.path.clone();
        self.transaction(|tx|{tx.execute("UPDATE players SET is_closer=0",[]).map_err(|e|StoreError::operation(OP,&path,e))?;let mut n=0;for(team,name)in rows{n+=tx.execute("UPDATE players SET is_closer=1 WHERE id=(SELECT id FROM players WHERE mlb_team=?1 AND LOWER(name)=LOWER(?2) AND eligible_positions LIKE '%RP%' GROUP BY LOWER(name),mlb_team HAVING COUNT(*)=1)",params![team,name]).map_err(|e|StoreError::operation(OP,&path,e))?;}tx.execute("UPDATE players SET is_closer=1 WHERE id IN (SELECT p.id FROM players p JOIN mlbam_season_stats s ON s.player_id=p.id AND s.stat_group='pitching' WHERE p.eligible_positions LIKE '%RP%' AND NOT EXISTS(SELECT 1 FROM players c WHERE c.mlb_team=p.mlb_team AND c.is_closer=1) AND s.sv=(SELECT MAX(s2.sv) FROM mlbam_season_stats s2 JOIN players p2 ON p2.id=s2.player_id WHERE p2.mlb_team=p.mlb_team AND s2.stat_group='pitching'))",[]).map_err(|e|StoreError::operation(OP,&path,e))?;Ok(n)})
    }
    /// Atomically replace one complete projection season.
    pub fn replace_projections(
        &mut self,
        season: i64,
        rows: &[ProjectionWrite],
    ) -> Result<usize, StoreError> {
        const OP: &str = "replace projections";
        if season <= 0 || rows.is_empty() {
            return Err(StoreError::invalid(
                OP,
                "a positive season and complete rows are required",
            ));
        }
        let (_, now) = self.captured_time(OP)?;
        let path = self.path.clone();
        self.transaction(|tx| {
            tx.execute("DELETE FROM player_projections WHERE season=?1", [season]).map_err(|e| StoreError::operation(OP, &path, e))?;
            let mut written = 0;
            for row in rows {
                let player_id = tx.query_row("SELECT id FROM players WHERE mlbam_id=?1 ORDER BY CASE WHEN mlbam_match_source='seed' THEN 0 ELSE 1 END DESC,id LIMIT 1", [row.mlbam_id], |r| r.get::<_, i64>(0)).optional().map_err(|e| StoreError::operation(OP, &path, e))?;
                let Some(player_id) = player_id else { continue };
                tx.execute("INSERT INTO player_projections VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)", params![player_id,season,row.source,row.stat_group,row.pa,row.ip,row.hr,row.r,row.rbi,row.sb,row.avg,row.obp,row.slg,row.era,row.whip,row.k,row.w,row.sv,row.bb,now]).map_err(|e| StoreError::operation(OP, &path, e))?;
                written += 1;
            }
            Ok(written)
        })
    }
}

use rusqlite::OptionalExtension;
