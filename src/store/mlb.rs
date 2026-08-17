//! MLB roster and season-stat persistence through normalized durable tables.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::domain::HitterAverage;
use crate::domain::clean_fantasy_team_name;

use super::{Store, StoreError, validate_identity};

/// One complete roster row awaiting replacement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RosterWrite {
    pub mlbam_id: i64,
    pub name: String,
    pub position: String,
    pub primary_type: String,
    pub status: String,
    pub jersey_number: String,
}

/// One stored roster row with display identity.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredRosterPlayer {
    pub mlbam_id: i64,
    pub name: String,
    pub position: String,
    pub primary_type: String,
    pub status: String,
    pub injury_status: String,
    pub is_closer: bool,
    pub jersey_number: String,
    pub eligible_positions: String,
    pub bat_side: String,
    pub pitch_hand: String,
    pub yahoo_rank: Option<i64>,
    pub owner: Option<String>,
    pub in_yahoo_pool: bool,
    pub plate_appearances: i64,
    pub on_base_percentage: f64,
    pub runs: i64,
    pub home_runs: i64,
    pub runs_batted_in: i64,
    pub stolen_bases: i64,
    pub batting_average: f64,
    pub innings_pitched: f64,
    pub quality_starts: i64,
    pub wins: i64,
    pub saves: i64,
    pub strikeouts: i64,
    pub earned_run_average: f64,
    pub whip: f64,
}

/// Counting inputs used to replace one player's season-stat role.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SeasonStatWrite {
    pub mlbam_id: i64,
    pub name: String,
    pub team_abbreviation: String,
    pub stat_group: String,
    pub games: i64,
    pub plate_appearances: i64,
    pub at_bats: i64,
    pub hits: i64,
    pub home_runs: i64,
    pub runs_batted_in: i64,
    pub runs: i64,
    pub stolen_bases: i64,
    pub walks: i64,
    pub hit_by_pitch: i64,
    pub total_bases: i64,
    pub wins: i64,
    pub saves: i64,
    pub holds: i64,
    pub strikeouts: i64,
    pub innings_outs: i64,
    pub games_started: i64,
    pub quality_starts: i64,
    pub hits_allowed: i64,
    pub earned_runs: i64,
    pub pitcher_walks: i64,
}

/// One active MLB player and the season usage required for waiver gating.
#[derive(Clone, Debug, PartialEq)]
pub struct WaiverCandidate {
    pub mlbam_id: i64,
    pub role: String,
    pub positions: String,
    pub plate_appearances: f64,
    pub innings_pitched: f64,
    pub games: i64,
    pub games_started: i64,
}

impl Store {
    /// Read durable local pitcher ownership keyed by folded full name.
    pub fn mlb_local_pitcher_ownership(
        &self,
        current_team_key: &str,
    ) -> Result<std::collections::BTreeMap<String, (bool, bool, bool)>, StoreError> {
        let mut statement = self.connection().prepare("SELECT LOWER(p.name),p.yahoo_player_id IS NOT NULL,EXISTS(SELECT 1 FROM yahoo_roster_slots ys WHERE ys.player_id=p.id),EXISTS(SELECT 1 FROM yahoo_roster_slots ys WHERE ys.player_id=p.id AND ys.team_key=?1) FROM players p WHERE p.position_type='P' ORDER BY LOWER(p.name),p.id").map_err(|error| StoreError::operation("read local pitcher ownership", &self.path, error))?;
        let rows = statement
            .query_map([current_team_key], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    (
                        row.get::<_, bool>(1)?,
                        row.get::<_, bool>(2)?,
                        row.get::<_, bool>(3)?,
                    ),
                ))
            })
            .map_err(|error| {
                StoreError::operation("read local pitcher ownership", &self.path, error)
            })?;
        rows.collect::<Result<_, _>>().map_err(|error| {
            StoreError::operation("read local pitcher ownership", &self.path, error)
        })
    }

    /// Read optional durable Yahoo rostered and MLB available player counts by club.
    pub fn mlb_local_player_counts(
        &self,
    ) -> Result<std::collections::BTreeMap<String, (i64, i64)>, StoreError> {
        let mut statement = self.connection().prepare(
            "WITH rostered AS (SELECT p.mlb_team team, COUNT(DISTINCT s.player_id) count FROM yahoo_roster_slots s JOIN players p ON p.id=s.player_id WHERE p.mlb_team IS NOT NULL GROUP BY p.mlb_team), total AS (SELECT p.mlb_team team, COUNT(DISTINCT m.player_id) count FROM mlbam_season_stats m JOIN players p ON p.id=m.player_id WHERE p.mlb_team IS NOT NULL GROUP BY p.mlb_team) SELECT total.team, COALESCE(rostered.count,0), MAX(total.count-COALESCE(rostered.count,0),0) FROM total LEFT JOIN rostered ON rostered.team=total.team WHERE EXISTS(SELECT 1 FROM yahoo_roster_slots) ORDER BY total.team"
        ).map_err(|error| StoreError::operation("read local MLB player counts", &self.path, error))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, (row.get(1)?, row.get(2)?)))
            })
            .map_err(|error| {
                StoreError::operation("read local MLB player counts", &self.path, error)
            })?;
        rows.collect::<Result<_, _>>().map_err(|error| {
            StoreError::operation("read local MLB player counts", &self.path, error)
        })
    }

    /// Replace one team's complete validated 40-man roster.
    pub fn replace_mlb_roster(
        &mut self,
        team: &str,
        rows: &[RosterWrite],
    ) -> Result<(), StoreError> {
        const OP: &str = "replace MLB roster";
        validate_identity(OP, "team abbreviation", team)?;
        let had_rows = self
            .connection()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM mlb_team_active_rosters WHERE team_abbr=?1)",
                [team],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| StoreError::operation(OP, &self.path, error))?;
        if rows.is_empty() && had_rows {
            return Err(StoreError::invalid(
                OP,
                "empty acquisition cannot replace a prior nonempty roster",
            ));
        }
        let mut keys = std::collections::BTreeSet::new();
        for row in rows {
            if row.mlbam_id <= 0
                || row.name.trim().is_empty()
                || !matches!(row.primary_type.as_str(), "H" | "P")
                || !keys.insert((row.mlbam_id, row.primary_type.clone()))
            {
                return Err(StoreError::invalid(
                    OP,
                    "roster rows require positive unique person-and-role identities",
                ));
            }
        }
        let (_, now) = self.captured_time(OP)?;
        let team = team.to_uppercase();
        let path = self.path.clone();
        self.transaction(|tx| {
            tx.execute("DELETE FROM mlb_team_active_rosters WHERE team_abbr=?1", [&team]).map_err(|error| StoreError::operation(OP, &path, error))?;
            for row in rows {
                let player_id: Option<i64> = tx.query_row("SELECT id FROM players WHERE mlbam_id=?1 AND position_type=?2 ORDER BY yahoo_player_id IS NULL, id LIMIT 1", params![row.mlbam_id, row.primary_type], |result| result.get(0)).optional().map_err(|error| StoreError::operation(OP, &path, error))?;
                if let Some(player_id) = player_id {
                    tx.execute("UPDATE players SET name=?2, mlb_team=?3, display_position=?4, jersey_number=?5, synced_at=?6 WHERE id=?1", params![player_id, row.name, team, row.position, nullable(&row.jersey_number), now]).map_err(|error| StoreError::operation(OP, &path, error))?;
                } else {
                    tx.execute("INSERT INTO players (mlbam_id,name,mlb_team,display_position,position_type,status,jersey_number,mlbam_match_source,mlbam_matched_at,synced_at) VALUES (?1,?2,?3,?4,?5,?6,?7,'40man',?8,?8)", params![row.mlbam_id,row.name,team,row.position,row.primary_type,row.status,nullable(&row.jersey_number),now]).map_err(|error| StoreError::operation(OP, &path, error))?;
                }
                tx.execute("INSERT INTO mlb_team_active_rosters (team_abbr,mlbam_id,primary_type,status,jersey_number,fetched_at) VALUES (?1,?2,?3,?4,?5,?6)", params![team,row.mlbam_id,row.primary_type,row.status,nullable(&row.jersey_number),now]).map_err(|error| StoreError::operation(OP, &path, error))?;
            }
            Ok(())
        })
    }

    /// Derive one hitter's rolling five-completed-season 162-game line.
    pub fn hitter_average(
        &self,
        mlbam_id: i64,
        current_season: i64,
    ) -> Result<Option<HitterAverage>, StoreError> {
        const OP: &str = "read hitter completed-season average";
        let values: (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) = self
            .connection()
            .query_row(
                "SELECT COALESCE(SUM(g),0),COALESCE(SUM(pa),0),COALESCE(SUM(r),0),COALESCE(SUM(hr),0),COALESCE(SUM(rbi),0),COALESCE(SUM(sb),0),COALESCE(SUM(h),0),COALESCE(SUM(ab),0),COALESCE(SUM(bb),0),COALESCE(SUM(hbp),0),COALESCE(SUM(tb),0) FROM mlbam_season_stats WHERE stat_group='hitting' AND season>=?2 AND season<?3 AND player_id=(SELECT id FROM players WHERE mlbam_id=?1 AND position_type IN ('H','B') ORDER BY CASE WHEN mlbam_match_source='seed' THEN 0 ELSE 1 END,id LIMIT 1)",
                (mlbam_id, current_season - 5, current_season),
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?,row.get(10)?)),
            )
            .map_err(|error| StoreError::operation(OP, &self.path, error))?;
        let (games, pa, runs, home_runs, rbi, stolen_bases, hits, at_bats, walks, hbp, total_bases) =
            values;
        if games == 0 || at_bats == 0 {
            return Ok(None);
        }
        let scale = |value: i64| ((value as f64 * 162.0 / games as f64) + 0.5) as i64;
        let avg = hits as f64 / at_bats as f64;
        let slg = total_bases as f64 / at_bats as f64;
        let denominator = at_bats + walks + hbp;
        let obp = if denominator == 0 {
            0.0
        } else {
            (hits + walks + hbp) as f64 / denominator as f64
        };
        Ok(Some(HitterAverage {
            plate_appearances: scale(pa),
            on_base_percentage: obp,
            on_base_plus_slugging: obp + slg,
            runs: scale(runs),
            home_runs: scale(home_runs),
            runs_batted_in: scale(rbi),
            stolen_bases: scale(stolen_bases),
            batting_average: avg,
        }))
    }

    /// Read one team's roster in stable role and name order.
    pub fn mlb_roster(&self, team: &str) -> Result<Vec<StoredRosterPlayer>, StoreError> {
        let mut statement = self.connection().prepare("SELECT r.mlbam_id,COALESCE(p.name,seed.name,''),COALESCE(p.display_position,seed.display_position,''),r.primary_type,r.status,COALESCE(p.status,''),COALESCE(p.is_closer,0),COALESCE(r.jersey_number,''),COALESCE(p.eligible_positions,''),COALESCE(p.bat_side,seed.bat_side,''),COALESCE(p.pitch_hand,seed.pitch_hand,''),p.yahoo_rank,(SELECT t.name FROM yahoo_roster_slots ys JOIN yahoo_teams t ON t.team_key=ys.team_key WHERE ys.player_id=p.id ORDER BY t.team_key LIMIT 1),p.yahoo_player_id IS NOT NULL,COALESCE(s.pa,0),COALESCE(s.obp,0),COALESCE(s.r,0),COALESCE(s.hr,0),COALESCE(s.rbi,0),COALESCE(s.sb,0),COALESCE(s.avg,0),COALESCE(s.ip,0),COALESCE(s.qs,0),COALESCE(s.w,0),COALESCE(s.sv,0),COALESCE(s.k,0),COALESCE(s.era,0),COALESCE(s.whip,0) FROM mlb_team_active_rosters r LEFT JOIN players p ON p.id=(SELECT p2.id FROM players p2 WHERE p2.mlbam_id=r.mlbam_id AND ((r.primary_type='H' AND p2.position_type IN ('H','B')) OR (r.primary_type='P' AND p2.position_type='P')) ORDER BY p2.yahoo_player_id IS NULL,CASE WHEN p2.mlbam_match_source='seed' THEN 0 ELSE 1 END,p2.id LIMIT 1) LEFT JOIN players seed ON seed.id=(SELECT p3.id FROM players p3 WHERE p3.mlbam_id=r.mlbam_id ORDER BY CASE WHEN p3.mlbam_match_source='seed' THEN 0 ELSE 1 END,p3.id LIMIT 1) LEFT JOIN mlbam_season_stats s ON s.player_id=COALESCE(seed.id,p.id) AND s.season=(SELECT MAX(season) FROM mlbam_season_stats) AND s.stat_group=CASE r.primary_type WHEN 'P' THEN 'pitching' ELSE 'hitting' END WHERE r.team_abbr=?1 ORDER BY CASE r.primary_type WHEN 'H' THEN 0 ELSE 1 END,CASE WHEN r.status='A' THEN 0 ELSE 1 END,CASE r.primary_type WHEN 'H' THEN -COALESCE(s.pa,0) ELSE -COALESCE(s.ip,0) END,COALESCE(p.name,seed.name,''),r.mlbam_id").map_err(|error| StoreError::operation("read MLB roster", &self.path, error))?;
        let rows = statement
            .query_map([team.to_uppercase()], |row| {
                Ok(StoredRosterPlayer {
                    mlbam_id: row.get(0)?,
                    name: row.get(1)?,
                    position: row.get(2)?,
                    primary_type: row.get(3)?,
                    status: row.get(4)?,
                    injury_status: row.get(5)?,
                    is_closer: row.get(6)?,
                    jersey_number: row.get(7)?,
                    eligible_positions: row.get(8)?,
                    bat_side: row.get(9)?,
                    pitch_hand: row.get(10)?,
                    yahoo_rank: row.get(11)?,
                    owner: row
                        .get::<_, Option<String>>(12)?
                        .map(|name| clean_fantasy_team_name(&name)),
                    in_yahoo_pool: row.get(13)?,
                    plate_appearances: row.get(14)?,
                    on_base_percentage: row.get(15)?,
                    runs: row.get(16)?,
                    home_runs: row.get(17)?,
                    runs_batted_in: row.get(18)?,
                    stolen_bases: row.get(19)?,
                    batting_average: row.get(20)?,
                    innings_pitched: row.get(21)?,
                    quality_starts: row.get(22)?,
                    wins: row.get(23)?,
                    saves: row.get(24)?,
                    strikeouts: row.get(25)?,
                    earned_run_average: row.get(26)?,
                    whip: row.get(27)?,
                })
            })
            .map_err(|error| StoreError::operation("read MLB roster", &self.path, error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| StoreError::operation("read MLB roster", &self.path, error))
    }

    /// Read active 26-man membership and season usage for waiver filtering.
    pub fn waiver_candidates(&self) -> Result<Vec<WaiverCandidate>, StoreError> {
        let mut statement = self.connection().prepare("SELECT r.mlbam_id,r.primary_type,COALESCE(MAX(p.eligible_positions),MAX(p.display_position),''),MAX(COALESCE(s.pa,0)),MAX(COALESCE(s.ip,0)),MAX(COALESCE(s.g,0)),MAX(COALESCE(s.gs,0)) FROM mlb_team_active_rosters r LEFT JOIN players p ON p.mlbam_id=r.mlbam_id AND ((r.primary_type='H' AND p.position_type IN ('H','B')) OR (r.primary_type='P' AND p.position_type='P')) LEFT JOIN mlbam_season_stats s ON s.player_id=p.id AND s.season=(SELECT MAX(season) FROM mlbam_season_stats) AND s.stat_group=CASE r.primary_type WHEN 'P' THEN 'pitching' ELSE 'hitting' END WHERE r.status='A' GROUP BY r.mlbam_id,r.primary_type ORDER BY r.primary_type,r.mlbam_id")
            .map_err(|error| StoreError::operation("prepare waiver candidates", &self.path, error))?;
        let rows = statement
            .query_map([], |row| {
                Ok(WaiverCandidate {
                    mlbam_id: row.get(0)?,
                    role: row.get(1)?,
                    positions: row.get(2)?,
                    plate_appearances: row.get(3)?,
                    innings_pitched: row.get(4)?,
                    games: row.get(5)?,
                    games_started: row.get(6)?,
                })
            })
            .map_err(|error| StoreError::operation("query waiver candidates", &self.path, error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| StoreError::operation("read waiver candidates", &self.path, error))
    }

    /// Replace one season's supplied MLB player-stat roles.
    pub fn replace_mlb_season_stats(
        &mut self,
        season: i64,
        rows: &[SeasonStatWrite],
    ) -> Result<(), StoreError> {
        const OP: &str = "replace MLB season stats";
        if season <= 0 || rows.is_empty() {
            return Err(StoreError::invalid(
                OP,
                "season and complete statistic rows are required",
            ));
        }
        let (_, now) = self.captured_time(OP)?;
        let path = self.path.clone();
        self.transaction(|tx| {
            let retained_quality_starts = rows
                .iter()
                .filter(|row| row.stat_group == "pitching" && row.quality_starts == 0)
                .filter_map(|row| {
                    tx.query_row(
                        "SELECT MAX(s.qs) FROM mlbam_season_stats s JOIN players p ON p.id=s.player_id WHERE p.mlbam_id=?1 AND s.season=?2 AND s.stat_group='pitching'",
                        params![row.mlbam_id, season],
                        |result| result.get::<_, Option<i64>>(0),
                    )
                    .ok()
                    .flatten()
                    .filter(|quality_starts| *quality_starts > 0)
                    .map(|quality_starts| (row.mlbam_id, quality_starts))
                })
                .collect::<std::collections::BTreeMap<_, _>>();
            let groups = rows.iter().map(|row| row.stat_group.as_str()).collect::<std::collections::BTreeSet<_>>();
            for group in groups {
                tx.execute("DELETE FROM mlbam_season_stats WHERE season=?1 AND stat_group=?2", params![season, group]).map_err(|error| StoreError::operation(OP, &path, error))?;
            }
            for row in rows {
                if row.mlbam_id <= 0 || !matches!(row.stat_group.as_str(), "hitting" | "pitching") { return Err(StoreError::invalid(OP, "positive MLBAM ID and recognized stat group are required")); }
                let role = if row.stat_group == "pitching" { "P" } else { "H" };
                let player_id: Option<i64> = tx.query_row("SELECT id FROM players WHERE mlbam_id=?1 AND position_type=?2 ORDER BY id LIMIT 1", params![row.mlbam_id, role], |result| result.get(0)).optional().map_err(|error| StoreError::operation(OP, &path, error))?;
                let player_id = if let Some(id) = player_id { id } else {
                    tx.execute("INSERT INTO players (mlbam_id,name,mlb_team,position_type,mlbam_match_source,mlbam_matched_at,synced_at) VALUES (?1,?2,?3,?4,'seed',?5,?5)", params![row.mlbam_id,row.name,row.team_abbreviation,role,now]).map_err(|error| StoreError::operation(OP, &path, error))?;
                    tx.last_insert_rowid()
                };
                let actual_ip = row.innings_outs as f64 / 3.0;
                let ip = display_innings(row.innings_outs);
                let era = ratio(9.0 * row.earned_runs as f64, actual_ip);
                let whip = ratio((row.pitcher_walks + row.hits_allowed) as f64, actual_ip);
                let quality_starts = retained_quality_starts.get(&row.mlbam_id).copied().unwrap_or(row.quality_starts);
                tx.execute("INSERT INTO mlbam_season_stats (player_id,season,stat_group,g,pa,ab,h,hr,rbi,r,sb,avg,obp,bb,hbp,tb,slg,ops,w,sv,hld,k,era,whip,ip,gs,qs,h_pit,er,bb_pit,synced_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31)", params![player_id,season,row.stat_group,row.games,row.plate_appearances,row.at_bats,row.hits,row.home_runs,row.runs_batted_in,row.runs,row.stolen_bases,ratio(row.hits as f64,row.at_bats as f64),ratio((row.hits+row.walks+row.hit_by_pitch) as f64,(row.at_bats+row.walks+row.hit_by_pitch) as f64),row.walks,row.hit_by_pitch,row.total_bases,ratio(row.total_bases as f64,row.at_bats as f64),ratio((row.hits+row.walks+row.hit_by_pitch) as f64,(row.at_bats+row.walks+row.hit_by_pitch) as f64)+ratio(row.total_bases as f64,row.at_bats as f64),row.wins,row.saves,row.holds,row.strikeouts,era,whip,ip,row.games_started,quality_starts,row.hits_allowed,row.earned_runs,row.pitcher_walks,now]).map_err(|error| StoreError::operation(OP, &path, error))?;
            }
            Ok(())
        })
    }
}

fn nullable(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}
fn ratio(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

fn display_innings(outs: i64) -> f64 {
    let whole = outs.div_euclid(3);
    let remainder = outs.rem_euclid(3);
    whole as f64 + remainder as f64 / 10.0
}
