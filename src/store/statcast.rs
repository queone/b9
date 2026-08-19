use rusqlite::{OptionalExtension, params};

use super::{Store, StoreError};

/// One normalized Baseball Savant season row keyed by MLB identity.
#[derive(Clone, Debug, PartialEq)]
pub struct StatcastWrite {
    pub mlbam_id: i64,
    pub season: i64,
    pub stat_group: String,
    pub plate_appearances: i64,
    pub batted_ball_events: i64,
    pub xwoba: Option<f64>,
    pub exit_velo_avg: Option<f64>,
    pub barrel_pct: Option<f64>,
    pub hard_hit_pct: Option<f64>,
    pub sprint_speed: Option<f64>,
    pub strikeout_pct: Option<f64>,
    pub walk_pct: Option<f64>,
    pub ops: Option<f64>,
    pub fastball_velo: Option<f64>,
    pub whiff_pct: Option<f64>,
    pub chase_pct: Option<f64>,
    pub gb_pct: Option<f64>,
}

impl Store {
    /// Atomically replace one complete Statcast season and group snapshot.
    pub fn replace_statcast_snapshot(
        &mut self,
        season: i64,
        group: &str,
        rows: &[StatcastWrite],
    ) -> Result<usize, StoreError> {
        if season <= 0 || !matches!(group, "batting" | "pitching") {
            return Err(StoreError::invalid(
                "replace Statcast snapshot",
                "season and stat group must be valid",
            ));
        }
        if rows
            .iter()
            .any(|row| row.mlbam_id <= 0 || row.season != season || row.stat_group != group)
        {
            return Err(StoreError::invalid(
                "replace Statcast snapshot",
                "every row must match the positive identity, season, and stat group",
            ));
        }
        let path = self.path.clone();
        let (_, now) = self.captured_time("replace Statcast snapshot")?;
        self.transaction(|transaction| {
            transaction.execute("DELETE FROM statcast_seasons WHERE season=?1 AND stat_group=?2", params![season, group]).map_err(|error| StoreError::operation("clear Statcast snapshot", &path, error))?;
            let mut written = 0;
            for row in rows {
                let player_id = transaction.query_row("SELECT id FROM players WHERE mlbam_id=?1 ORDER BY CASE WHEN mlbam_match_source='seed' THEN 0 ELSE 1 END DESC,yahoo_player_id IS NULL,id LIMIT 1", [row.mlbam_id], |result| result.get::<_, i64>(0)).optional().map_err(|error| StoreError::operation("resolve Statcast player identity", &path, error))?;
                let Some(player_id) = player_id else { continue; };
                transaction.execute("INSERT INTO statcast_seasons(player_id,season,stat_group,pa,bbe,xwoba,exit_velo_avg,barrel_pct,hard_hit_pct,sprint_speed,strikeout_pct,walk_pct,ops,fastball_velo,whiff_pct,chase_pct,gb_pct,fetched_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)", params![player_id,season,group,row.plate_appearances,row.batted_ball_events,row.xwoba,row.exit_velo_avg,row.barrel_pct,row.hard_hit_pct,row.sprint_speed,row.strikeout_pct,row.walk_pct,row.ops,row.fastball_velo,row.whiff_pct,row.chase_pct,row.gb_pct,now]).map_err(|error| StoreError::operation("write Statcast snapshot", &path, error))?;
                written += 1;
            }
            Ok(written)
        })
    }
}
