use rusqlite::{OptionalExtension, params};

use super::{Store, StoreError};

#[derive(Clone, Debug, PartialEq)]
pub struct FangraphsBattedBallWrite {
    pub mlbam_id: i64,
    pub season: i64,
    pub fb_pct: f64,
    pub hr_fb_pct: f64,
}

impl Store {
    /// Atomically replace one complete FanGraphs batted-ball season.
    pub fn replace_fangraphs_batted_ball(
        &mut self,
        season: i64,
        rows: &[FangraphsBattedBallWrite],
    ) -> Result<usize, StoreError> {
        const OP: &str = "replace FanGraphs batted-ball snapshot";
        if season <= 0 || rows.is_empty() {
            return Err(StoreError::invalid(
                OP,
                "a positive season and complete rows are required",
            ));
        }
        let (_, now) = self.captured_time(OP)?;
        let path = self.path.clone();
        self.transaction(|tx| {
            tx.execute("DELETE FROM fangraphs_batted_ball WHERE season=?1", [season]).map_err(|e| StoreError::operation(OP, &path, e))?;
            let mut written = 0;
            for row in rows {
                let id = tx.query_row("SELECT id FROM players WHERE mlbam_id=?1 ORDER BY CASE WHEN mlbam_match_source='seed' THEN 0 ELSE 1 END DESC,id LIMIT 1", [row.mlbam_id], |r| r.get::<_, i64>(0)).optional().map_err(|e| StoreError::operation(OP, &path, e))?;
                let Some(id) = id else { continue };
                tx.execute("INSERT INTO fangraphs_batted_ball VALUES(?1,?2,?3,?4,?5)", params![id,season,row.fb_pct,row.hr_fb_pct,now]).map_err(|e| StoreError::operation(OP, &path, e))?;
                written += 1;
            }
            Ok(written)
        })
    }
}
