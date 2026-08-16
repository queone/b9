//! Normalized fantasy-league persistence on the existing version-one schema.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{OptionalExtension, params};

use crate::domain::{FantasyPlayer, FantasyRosterSlot, FantasyTeam, League};

use super::{Store, StoreError, validate_identity};

/// One scoring-category persistence row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryWrite {
    pub stat_id: i64,
    pub abbreviation: String,
    pub name: String,
    pub sort_order: i32,
    pub display_only: bool,
    pub sequence: i64,
}

/// One roster-position persistence row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PositionWrite {
    pub position: String,
    pub count: i64,
}

/// One complete stable Yahoo league snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct FantasySnapshotWrite {
    pub league: League,
    pub current_week: Option<i32>,
    pub categories: Vec<CategoryWrite>,
    pub positions: Vec<PositionWrite>,
    pub teams: Vec<FantasyTeam>,
    pub players: Vec<FantasyPlayer>,
    pub slots: Vec<FantasyRosterSlot>,
}

/// One persisted team read model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredFantasyTeam {
    pub team_key: String,
    pub name: String,
    pub team_id: i64,
}

/// One candidate MLB identity for a Yahoo player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityCandidate {
    pub mlbam_id: i64,
    pub name: String,
    pub team: String,
    pub role: String,
}

impl Store {
    /// Replace one complete league snapshot transactionally.
    pub fn replace_fantasy_snapshot(
        &mut self,
        snapshot: &FantasySnapshotWrite,
    ) -> Result<(), StoreError> {
        validate_snapshot(snapshot)?;
        let (_, captured_at) = self.captured_time("replace fantasy snapshot")?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction.execute(
                "INSERT INTO yahoo_leagues (league_key,name,season,num_teams,scoring_type,current_week,synced_at) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(league_key) DO UPDATE SET name=excluded.name,season=excluded.season,num_teams=excluded.num_teams,scoring_type=excluded.scoring_type,current_week=excluded.current_week,synced_at=excluded.synced_at",
                params![snapshot.league.league_key, snapshot.league.name, snapshot.league.season, snapshot.league.num_teams, snapshot.league.scoring_type.to_string(), snapshot.current_week, captured_at],
            ).map_err(|error| StoreError::operation("upsert Yahoo league", &path, error))?;

            transaction.execute("DELETE FROM yahoo_stat_categories WHERE league_key=?1", [&snapshot.league.league_key])
                .map_err(|error| StoreError::operation("replace Yahoo categories", &path, error))?;
            for row in &snapshot.categories {
                transaction.execute("INSERT INTO yahoo_stat_categories (league_key,stat_id,abbr,name,sort_order,display_only,seq) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                    params![snapshot.league.league_key,row.stat_id,row.abbreviation,row.name,row.sort_order,i64::from(row.display_only),row.sequence])
                    .map_err(|error| StoreError::operation("insert Yahoo category", &path, error))?;
            }
            transaction.execute("DELETE FROM yahoo_roster_positions WHERE league_key=?1", [&snapshot.league.league_key])
                .map_err(|error| StoreError::operation("replace Yahoo positions", &path, error))?;
            for row in &snapshot.positions {
                transaction.execute("INSERT INTO yahoo_roster_positions (league_key,position,count) VALUES (?1,?2,?3)",
                    params![snapshot.league.league_key,row.position,row.count])
                    .map_err(|error| StoreError::operation("insert Yahoo position", &path, error))?;
            }
            for team in &snapshot.teams {
                transaction.execute("INSERT INTO yahoo_teams (team_key,league_key,team_id,name,manager_nickname,synced_at) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(team_key) DO UPDATE SET league_key=excluded.league_key,team_id=excluded.team_id,name=excluded.name,manager_nickname=excluded.manager_nickname,synced_at=excluded.synced_at",
                    params![team.team_key,team.league_key,team.team_id,team.name,team.manager_name,captured_at])
                    .map_err(|error| StoreError::operation("upsert Yahoo team", &path, error))?;
            }
            for player in &snapshot.players {
                let eligible = player.eligible_positions.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
                transaction.execute("INSERT INTO players (yahoo_player_id,name,mlb_team,display_position,position_type,eligible_positions,status,percent_owned,yahoo_rank,synced_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(yahoo_player_id) DO UPDATE SET name=excluded.name,mlb_team=excluded.mlb_team,display_position=excluded.display_position,position_type=excluded.position_type,eligible_positions=excluded.eligible_positions,status=excluded.status,percent_owned=excluded.percent_owned,yahoo_rank=excluded.yahoo_rank,synced_at=excluded.synced_at",
                    params![player.yahoo_player_id,player.name,player.mlb_team,player.display_position,player.position_type,eligible,player.injury_status,player.percent_owned,player.yahoo_rank,captured_at])
                    .map_err(|error| StoreError::operation("upsert Yahoo player", &path, error))?;
            }
            transaction.execute("DELETE FROM yahoo_roster_slots WHERE team_key IN (SELECT team_key FROM yahoo_teams WHERE league_key=?1)", [&snapshot.league.league_key])
                .map_err(|error| StoreError::operation("replace Yahoo roster slots", &path, error))?;
            let mut old_team_statement = transaction.prepare("SELECT team_key FROM yahoo_teams WHERE league_key=?1 ORDER BY team_key")
                .map_err(|error| StoreError::operation("prepare stale Yahoo teams", &path, error))?;
            let old_team_rows = old_team_statement.query_map([&snapshot.league.league_key], |row| row.get::<_, String>(0))
                .map_err(|error| StoreError::operation("query stale Yahoo teams", &path, error))?;
            let old_team_keys = old_team_rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| StoreError::operation("read stale Yahoo teams", &path, error))?;
            drop(old_team_statement);
            for team_key in old_team_keys {
                if !snapshot.teams.iter().any(|team| team.team_key == team_key) {
                    transaction.execute("DELETE FROM yahoo_teams WHERE team_key=?1", [&team_key])
                        .map_err(|error| StoreError::operation("delete stale Yahoo team", &path, error))?;
                }
            }
            for slot in &snapshot.slots {
                let player_id: i64 = transaction.query_row("SELECT id FROM players WHERE yahoo_player_id=?1", [slot.yahoo_player_id], |row| row.get(0))
                    .map_err(|error| StoreError::operation("resolve Yahoo roster player", &path, error))?;
                transaction.execute("INSERT INTO yahoo_roster_slots (team_key,player_id,slot_position,synced_at) VALUES (?1,?2,?3,?4)",
                    params![slot.team_key,player_id,slot.slot_position.to_string(),captured_at])
                    .map_err(|error| StoreError::operation("insert Yahoo roster slot", &path, error))?;
            }
            Ok(())
        })
    }

    /// Read teams for one league in stable provider-key order.
    pub fn fantasy_teams(&self, league_key: &str) -> Result<Vec<StoredFantasyTeam>, StoreError> {
        validate_identity("read fantasy teams", "league key", league_key)?;
        let mut statement = self.connection().prepare("SELECT team_key,name,team_id FROM yahoo_teams WHERE league_key=?1 ORDER BY team_key")
            .map_err(|error| StoreError::operation("prepare fantasy teams", &self.path, error))?;
        let rows = statement
            .query_map([league_key], |row| {
                Ok(StoredFantasyTeam {
                    team_key: row.get(0)?,
                    name: row.get(1)?,
                    team_id: row.get(2)?,
                })
            })
            .map_err(|error| StoreError::operation("query fantasy teams", &self.path, error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| StoreError::operation("read fantasy teams", &self.path, error))
    }

    /// Read the persisted current week for one league.
    pub fn fantasy_current_week(&self, league_key: &str) -> Result<Option<i32>, StoreError> {
        validate_identity("read fantasy current week", "league key", league_key)?;
        self.connection()
            .query_row(
                "SELECT current_week FROM yahoo_leagues WHERE league_key=?1",
                [league_key],
                |row| row.get(0),
            )
            .optional()
            .map(|value| value.flatten())
            .map_err(|error| StoreError::operation("read fantasy current week", &self.path, error))
    }

    /// Reconcile missing Yahoo player identities against unique MLB candidates.
    pub fn reconcile_mlb_identities(
        &mut self,
        candidates: &[IdentityCandidate],
    ) -> Result<usize, StoreError> {
        let mut groups: BTreeMap<(String, String, String), Vec<i64>> = BTreeMap::new();
        for candidate in candidates {
            if candidate.mlbam_id <= 0 {
                continue;
            }
            groups
                .entry(identity_key(
                    &candidate.name,
                    &candidate.team,
                    &candidate.role,
                ))
                .or_default()
                .push(candidate.mlbam_id);
        }
        let path = self.path.clone();
        self.transaction(|transaction| {
            let mut statement = transaction.prepare("SELECT id,name,COALESCE(mlb_team,''),COALESCE(position_type,'') FROM players WHERE yahoo_player_id IS NOT NULL AND mlbam_id IS NULL ORDER BY id")
                .map_err(|error| StoreError::operation("prepare identity reconciliation", &path, error))?;
            let rows = statement.query_map([], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?)))
                .map_err(|error| StoreError::operation("query identity reconciliation", &path, error))?;
            let players = rows.collect::<Result<Vec<_>,_>>().map_err(|error| StoreError::operation("read identity reconciliation", &path, error))?;
            drop(statement);
            let mut updated = 0;
            for (id,name,team,role) in players {
                if let Some(ids) = groups.get(&identity_key(&name,&team,&role)).filter(|ids| ids.len() == 1) {
                    transaction.execute("UPDATE players SET mlbam_id=?1,mlbam_match_source='name+team+pos' WHERE id=?2 AND mlbam_id IS NULL", params![ids[0],id])
                        .map_err(|error| StoreError::operation("update identity reconciliation", &path, error))?;
                    updated += 1;
                }
            }
            Ok(updated)
        })
    }
}

fn validate_snapshot(snapshot: &FantasySnapshotWrite) -> Result<(), StoreError> {
    validate_identity(
        "replace fantasy snapshot",
        "league key",
        &snapshot.league.league_key,
    )?;
    if snapshot.teams.is_empty()
        || snapshot.teams.len() != snapshot.league.num_teams as usize
        || snapshot.players.is_empty()
        || snapshot.slots.is_empty()
    {
        return Err(StoreError::invalid(
            "replace fantasy snapshot",
            "teams, players, and slots must be complete",
        ));
    }
    let team_keys = snapshot
        .teams
        .iter()
        .map(|team| team.team_key.as_str())
        .collect::<BTreeSet<_>>();
    if team_keys.len() != snapshot.teams.len()
        || snapshot
            .teams
            .iter()
            .any(|team| team.league_key != snapshot.league.league_key)
    {
        return Err(StoreError::invalid(
            "replace fantasy snapshot",
            "team set is duplicated or outside the league",
        ));
    }
    let player_ids = snapshot
        .players
        .iter()
        .map(|player| player.yahoo_player_id)
        .collect::<BTreeSet<_>>();
    if player_ids.len() != snapshot.players.len()
        || snapshot.slots.iter().any(|slot| {
            !team_keys.contains(slot.team_key.as_str())
                || !player_ids.contains(&slot.yahoo_player_id)
        })
        || team_keys
            .iter()
            .any(|team_key| !snapshot.slots.iter().any(|slot| slot.team_key == *team_key))
    {
        return Err(StoreError::invalid(
            "replace fantasy snapshot",
            "roster ownership is incomplete or mismatched",
        ));
    }
    Ok(())
}

fn identity_key(name: &str, team: &str, role: &str) -> (String, String, String) {
    let normalized = |value: &str| {
        value
            .chars()
            .filter(|character| character.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    (
        normalized(name),
        team.trim().to_ascii_uppercase(),
        role.trim().to_ascii_uppercase(),
    )
}
