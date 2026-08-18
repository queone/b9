//! Normalized fantasy-league persistence on the existing version-one schema.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{OptionalExtension, params};

use crate::domain::{
    FantasyPlayer, FantasyRosterSlot, FantasyTeam, League, StoredFantasyPlayer,
    clean_fantasy_team_name,
};

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
    pub manager_name: String,
    pub team_id: i64,
    pub waiver_priority: i64,
    pub faab_balance: i64,
    pub wins: i64,
    pub losses: i64,
    pub ties: i64,
    pub moves: i64,
    pub rank: i64,
}

/// One persisted Yahoo scoring category used for ordered weekly output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredFantasyCategory {
    pub stat_id: i64,
    pub abbreviation: String,
    pub sequence: i64,
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
    /// Replace one complete authenticated Yahoo category collection.
    pub fn replace_authenticated_categories(
        &mut self,
        league_key: &str,
        rows: &[CategoryWrite],
    ) -> Result<(), StoreError> {
        validate_identity(
            "replace authenticated Yahoo categories",
            "league key",
            league_key,
        )?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction.execute("DELETE FROM yahoo_stat_categories WHERE league_key=?1", [league_key]).map_err(|error| StoreError::operation("replace authenticated Yahoo categories", &path, error))?;
            for row in rows {
                transaction.execute("INSERT INTO yahoo_stat_categories (league_key,stat_id,abbr,name,sort_order,display_only,seq) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![league_key,row.stat_id,row.abbreviation,row.name,row.sort_order,i64::from(row.display_only),row.sequence]).map_err(|error| StoreError::operation("insert authenticated Yahoo category", &path, error))?;
            }
            Ok(())
        })
    }

    /// Merge authenticated-only Yahoo team fields.
    pub fn merge_authenticated_teams(&mut self, teams: &[FantasyTeam]) -> Result<(), StoreError> {
        let (_, captured_at) = self.captured_time("merge authenticated Yahoo teams")?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            for team in teams {
                transaction.execute("UPDATE yahoo_teams SET waiver_priority=?1,faab_balance=?2,moves=?3,synced_at=?4 WHERE team_key=?5", params![team.waiver_priority,team.faab_balance,team.moves,captured_at,team.team_key]).map_err(|error| StoreError::operation("merge authenticated Yahoo team", &path, error))?;
            }
            Ok(())
        })
    }

    /// Merge authenticated-only Yahoo player metadata.
    pub fn merge_authenticated_players(
        &mut self,
        players: &[FantasyPlayer],
    ) -> Result<(), StoreError> {
        let (_, captured_at) = self.captured_time("merge authenticated Yahoo players")?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            for player in players {
                let eligible = player.eligible_positions.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
                transaction.execute("INSERT INTO players (yahoo_player_id,name,mlb_team,display_position,position_type,eligible_positions,status,percent_owned,yahoo_rank,synced_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(yahoo_player_id) DO UPDATE SET status=excluded.status,percent_owned=excluded.percent_owned,yahoo_rank=COALESCE(excluded.yahoo_rank,players.yahoo_rank),synced_at=excluded.synced_at", params![player.yahoo_player_id,player.name,player.mlb_team,player.display_position,player.position_type,eligible,player.injury_status,player.percent_owned,player.yahoo_rank,captured_at]).map_err(|error| StoreError::operation("merge authenticated Yahoo player", &path, error))?;
            }
            Ok(())
        })
    }

    /// Replace one complete authenticated Yahoo free-agent collection.
    pub fn replace_authenticated_free_agents(
        &mut self,
        league_key: &str,
        players: &[FantasyPlayer],
    ) -> Result<(), StoreError> {
        self.merge_authenticated_players(players)?;
        let (_, captured_at) = self.captured_time("replace authenticated Yahoo free agents")?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction.execute("DELETE FROM yahoo_free_agents WHERE league_key=?1", [league_key]).map_err(|error| StoreError::operation("replace authenticated Yahoo free agents", &path, error))?;
            for player in players {
                let player_id: i64 = transaction.query_row("SELECT id FROM players WHERE yahoo_player_id=?1", [player.yahoo_player_id], |row| row.get(0)).map_err(|error| StoreError::operation("resolve authenticated Yahoo free agent", &path, error))?;
                transaction.execute("INSERT INTO yahoo_free_agents (league_key,player_id,synced_at) VALUES (?1,?2,?3)", params![league_key,player_id,captured_at]).map_err(|error| StoreError::operation("insert authenticated Yahoo free agent", &path, error))?;
            }
            Ok(())
        })
    }

    /// Merge one complete public Yahoo roster snapshot without erasing supplemental fields.
    pub fn merge_public_fantasy_snapshot(
        &mut self,
        snapshot: &FantasySnapshotWrite,
    ) -> Result<(), StoreError> {
        validate_snapshot(snapshot)?;
        let (_, captured_at) = self.captured_time("merge public fantasy snapshot")?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction.execute(
                "INSERT INTO yahoo_leagues (league_key,name,season,num_teams,scoring_type,current_week,synced_at) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(league_key) DO UPDATE SET name=excluded.name,season=excluded.season,num_teams=excluded.num_teams,scoring_type=excluded.scoring_type,current_week=excluded.current_week,synced_at=excluded.synced_at",
                params![snapshot.league.league_key, snapshot.league.name, snapshot.league.season, snapshot.league.num_teams, snapshot.league.scoring_type.to_string(), snapshot.current_week, captured_at],
            ).map_err(|error| StoreError::operation("upsert public Yahoo league", &path, error))?;

            transaction.execute("DELETE FROM yahoo_roster_positions WHERE league_key=?1", [&snapshot.league.league_key])
                .map_err(|error| StoreError::operation("replace public Yahoo positions", &path, error))?;
            for row in &snapshot.positions {
                transaction.execute("INSERT INTO yahoo_roster_positions (league_key,position,count) VALUES (?1,?2,?3)",
                    params![snapshot.league.league_key,row.position,row.count])
                    .map_err(|error| StoreError::operation("insert public Yahoo position", &path, error))?;
            }
            for team in &snapshot.teams {
                transaction.execute("INSERT INTO yahoo_teams (team_key,league_key,team_id,name,manager_nickname,waiver_priority,faab_balance,wins,losses,ties,moves,rank,synced_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13) ON CONFLICT(team_key) DO UPDATE SET league_key=excluded.league_key,team_id=excluded.team_id,name=excluded.name,manager_nickname=excluded.manager_nickname,waiver_priority=excluded.waiver_priority,faab_balance=excluded.faab_balance,wins=excluded.wins,losses=excluded.losses,ties=excluded.ties,moves=excluded.moves,rank=excluded.rank,synced_at=excluded.synced_at",
                    params![team.team_key,team.league_key,team.team_id,team.name,team.manager_name,team.waiver_priority,team.faab_balance,team.wins,team.losses,team.ties,team.moves,team.rank,captured_at])
                    .map_err(|error| StoreError::operation("upsert public Yahoo team", &path, error))?;
            }
            for player in &snapshot.players {
                let eligible = player.eligible_positions.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
                transaction.execute("INSERT INTO players (yahoo_player_id,name,mlb_team,display_position,position_type,eligible_positions,status,percent_owned,yahoo_rank,synced_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(yahoo_player_id) DO UPDATE SET name=excluded.name,mlb_team=excluded.mlb_team,display_position=excluded.display_position,position_type=excluded.position_type,eligible_positions=excluded.eligible_positions,status=CASE WHEN players.status NOT IN ('','IL') AND excluded.status IN ('','IL') THEN players.status ELSE excluded.status END,yahoo_rank=COALESCE(excluded.yahoo_rank,players.yahoo_rank),synced_at=excluded.synced_at",
                    params![player.yahoo_player_id,player.name,player.mlb_team,player.display_position,player.position_type,eligible,player.injury_status,player.percent_owned,player.yahoo_rank,captured_at])
                    .map_err(|error| StoreError::operation("upsert public Yahoo player", &path, error))?;
            }
            transaction.execute("DELETE FROM yahoo_roster_slots WHERE team_key IN (SELECT team_key FROM yahoo_teams WHERE league_key=?1)", [&snapshot.league.league_key])
                .map_err(|error| StoreError::operation("replace public Yahoo roster slots", &path, error))?;
            let current_team_keys = snapshot.teams.iter().map(|team| team.team_key.as_str()).collect::<BTreeSet<_>>();
            let mut stale_statement = transaction.prepare("SELECT team_key FROM yahoo_teams WHERE league_key=?1")
                .map_err(|error| StoreError::operation("prepare stale public Yahoo teams", &path, error))?;
            let stale_keys = stale_statement.query_map([&snapshot.league.league_key], |row| row.get::<_, String>(0))
                .map_err(|error| StoreError::operation("query stale public Yahoo teams", &path, error))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| StoreError::operation("read stale public Yahoo teams", &path, error))?;
            drop(stale_statement);
            for team_key in stale_keys.into_iter().filter(|key| !current_team_keys.contains(key.as_str())) {
                transaction.execute("DELETE FROM yahoo_teams WHERE team_key=?1", [&team_key])
                    .map_err(|error| StoreError::operation("delete stale public Yahoo team", &path, error))?;
            }
            for slot in &snapshot.slots {
                let player_id: i64 = transaction.query_row("SELECT id FROM players WHERE yahoo_player_id=?1", [slot.yahoo_player_id], |row| row.get(0))
                    .map_err(|error| StoreError::operation("resolve public Yahoo roster player", &path, error))?;
                transaction.execute("INSERT INTO yahoo_roster_slots (team_key,player_id,slot_position,synced_at) VALUES (?1,?2,?3,?4)",
                    params![slot.team_key,player_id,slot.slot_position.to_string(),captured_at])
                    .map_err(|error| StoreError::operation("insert public Yahoo roster slot", &path, error))?;
            }
            Ok(())
        })
    }

    /// Merge one complete authenticated Yahoo supplement without replacing public-owned fields.
    pub fn merge_authenticated_fantasy_supplement(
        &mut self,
        snapshot: &FantasySnapshotWrite,
    ) -> Result<(), StoreError> {
        validate_snapshot(snapshot)?;
        let (_, captured_at) = self.captured_time("merge authenticated fantasy supplement")?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction.execute("DELETE FROM yahoo_stat_categories WHERE league_key=?1", [&snapshot.league.league_key])
                .map_err(|error| StoreError::operation("replace authenticated Yahoo categories", &path, error))?;
            for row in &snapshot.categories {
                transaction.execute("INSERT INTO yahoo_stat_categories (league_key,stat_id,abbr,name,sort_order,display_only,seq) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                    params![snapshot.league.league_key,row.stat_id,row.abbreviation,row.name,row.sort_order,i64::from(row.display_only),row.sequence])
                    .map_err(|error| StoreError::operation("insert authenticated Yahoo category", &path, error))?;
            }
            for team in &snapshot.teams {
                transaction.execute("UPDATE yahoo_teams SET waiver_priority=?1,faab_balance=?2,moves=?3,synced_at=?4 WHERE team_key=?5",
                    params![team.waiver_priority,team.faab_balance,team.moves,captured_at,team.team_key])
                    .map_err(|error| StoreError::operation("merge authenticated Yahoo team", &path, error))?;
            }
            for player in &snapshot.players {
                let eligible = player.eligible_positions.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
                transaction.execute("INSERT INTO players (yahoo_player_id,name,mlb_team,display_position,position_type,eligible_positions,status,percent_owned,yahoo_rank,synced_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(yahoo_player_id) DO UPDATE SET status=excluded.status,percent_owned=excluded.percent_owned,yahoo_rank=COALESCE(excluded.yahoo_rank,players.yahoo_rank),synced_at=excluded.synced_at",
                    params![player.yahoo_player_id,player.name,player.mlb_team,player.display_position,player.position_type,eligible,player.injury_status,player.percent_owned,player.yahoo_rank,captured_at])
                    .map_err(|error| StoreError::operation("merge authenticated Yahoo player", &path, error))?;
            }
            transaction.execute("DELETE FROM yahoo_free_agents WHERE league_key=?1", [&snapshot.league.league_key])
                .map_err(|error| StoreError::operation("replace authenticated Yahoo free agents", &path, error))?;
            let rostered = snapshot.slots.iter().map(|slot| slot.yahoo_player_id).collect::<BTreeSet<_>>();
            for player in snapshot.players.iter().filter(|player| !rostered.contains(&player.yahoo_player_id)) {
                let player_id: i64 = transaction.query_row("SELECT id FROM players WHERE yahoo_player_id=?1", [player.yahoo_player_id], |row| row.get(0))
                    .map_err(|error| StoreError::operation("resolve authenticated Yahoo free agent", &path, error))?;
                transaction.execute("INSERT INTO yahoo_free_agents (league_key,player_id,synced_at) VALUES (?1,?2,?3)", params![snapshot.league.league_key,player_id,captured_at])
                    .map_err(|error| StoreError::operation("insert authenticated Yahoo free agent", &path, error))?;
            }
            Ok(())
        })
    }

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
                transaction.execute("INSERT INTO yahoo_teams (team_key,league_key,team_id,name,manager_nickname,waiver_priority,faab_balance,wins,losses,ties,moves,rank,synced_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13) ON CONFLICT(team_key) DO UPDATE SET league_key=excluded.league_key,team_id=excluded.team_id,name=excluded.name,manager_nickname=excluded.manager_nickname,waiver_priority=excluded.waiver_priority,faab_balance=excluded.faab_balance,wins=excluded.wins,losses=excluded.losses,ties=excluded.ties,moves=excluded.moves,rank=excluded.rank,synced_at=excluded.synced_at",
                    params![team.team_key,team.league_key,team.team_id,team.name,team.manager_name,team.waiver_priority,team.faab_balance,team.wins,team.losses,team.ties,team.moves,team.rank,captured_at])
                    .map_err(|error| StoreError::operation("upsert Yahoo team", &path, error))?;
            }
            for player in &snapshot.players {
                let eligible = player.eligible_positions.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
                transaction.execute("INSERT INTO players (yahoo_player_id,name,mlb_team,display_position,position_type,eligible_positions,status,percent_owned,yahoo_rank,synced_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(yahoo_player_id) DO UPDATE SET name=excluded.name,mlb_team=excluded.mlb_team,display_position=excluded.display_position,position_type=excluded.position_type,eligible_positions=excluded.eligible_positions,status=excluded.status,percent_owned=excluded.percent_owned,yahoo_rank=COALESCE(excluded.yahoo_rank,players.yahoo_rank),synced_at=excluded.synced_at",
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
            transaction
                .execute(
                    "DELETE FROM yahoo_free_agents WHERE league_key=?1",
                    [&snapshot.league.league_key],
                )
                .map_err(|error| StoreError::operation("replace Yahoo free agents", &path, error))?;
            let rostered = snapshot
                .slots
                .iter()
                .map(|slot| slot.yahoo_player_id)
                .collect::<BTreeSet<_>>();
            for player in snapshot
                .players
                .iter()
                .filter(|player| !rostered.contains(&player.yahoo_player_id))
            {
                let player_id: i64 = transaction
                    .query_row(
                        "SELECT id FROM players WHERE yahoo_player_id=?1",
                        [player.yahoo_player_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| StoreError::operation("resolve Yahoo free agent", &path, error))?;
                transaction.execute("INSERT INTO yahoo_free_agents (league_key,player_id,synced_at) VALUES (?1,?2,?3)", params![snapshot.league.league_key,player_id,captured_at])
                    .map_err(|error| StoreError::operation("insert Yahoo free agent", &path, error))?;
            }
            Ok(())
        })
    }

    /// Read teams for one league in stable provider-key order.
    pub fn fantasy_teams(&self, league_key: &str) -> Result<Vec<StoredFantasyTeam>, StoreError> {
        validate_identity("read fantasy teams", "league key", league_key)?;
        let mut statement = self.connection().prepare("SELECT team_key,name,COALESCE(manager_nickname,''),team_id,COALESCE(waiver_priority,0),COALESCE(faab_balance,0),COALESCE(wins,0),COALESCE(losses,0),COALESCE(ties,0),COALESCE(moves,0),COALESCE(rank,0) FROM yahoo_teams WHERE league_key=?1 ORDER BY CASE WHEN COALESCE(rank,0)>0 THEN rank ELSE 999999 END,team_key")
            .map_err(|error| StoreError::operation("prepare fantasy teams", &self.path, error))?;
        let rows = statement
            .query_map([league_key], |row| {
                Ok(StoredFantasyTeam {
                    team_key: row.get(0)?,
                    name: clean_fantasy_team_name(&row.get::<_, String>(1)?),
                    manager_name: row.get(2)?,
                    team_id: row.get(3)?,
                    waiver_priority: row.get(4)?,
                    faab_balance: row.get(5)?,
                    wins: row.get(6)?,
                    losses: row.get(7)?,
                    ties: row.get(8)?,
                    moves: row.get(9)?,
                    rank: row.get(10)?,
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

    /// Read the persisted season for one league.
    pub fn fantasy_season(&self, league_key: &str) -> Result<Option<i64>, StoreError> {
        validate_identity("read fantasy season", "league key", league_key)?;
        self.connection()
            .query_row(
                "SELECT season FROM yahoo_leagues WHERE league_key=?1",
                [league_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| StoreError::operation("read fantasy season", &self.path, error))
    }

    /// Read scoring categories in the league-defined display order.
    pub fn fantasy_categories(
        &self,
        league_key: &str,
    ) -> Result<Vec<StoredFantasyCategory>, StoreError> {
        validate_identity("read fantasy categories", "league key", league_key)?;
        let mut statement = self
            .connection()
            .prepare(
                "SELECT stat_id,abbr,seq FROM yahoo_stat_categories WHERE league_key=?1 AND display_only=0 ORDER BY seq,stat_id",
            )
            .map_err(|error| StoreError::operation("prepare fantasy categories", &self.path, error))?;
        let rows = statement
            .query_map([league_key], |row| {
                Ok(StoredFantasyCategory {
                    stat_id: row.get(0)?,
                    abbreviation: row.get(1)?,
                    sequence: row.get(2)?,
                })
            })
            .map_err(|error| {
                StoreError::operation("query fantasy categories", &self.path, error)
            })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| StoreError::operation("read fantasy categories", &self.path, error))
    }

    /// Read required roster positions in stable league display order.
    pub fn fantasy_positions(&self, league_key: &str) -> Result<Vec<PositionWrite>, StoreError> {
        validate_identity("read fantasy positions", "league key", league_key)?;
        let mut statement = self.connection().prepare("SELECT position,count FROM yahoo_roster_positions WHERE league_key=?1 ORDER BY position")
            .map_err(|error| StoreError::operation("prepare fantasy positions", &self.path, error))?;
        let rows = statement
            .query_map([league_key], |row| {
                Ok(PositionWrite {
                    position: row.get(0)?,
                    count: row.get(1)?,
                })
            })
            .map_err(|error| StoreError::operation("query fantasy positions", &self.path, error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| StoreError::operation("read fantasy positions", &self.path, error))
    }

    /// Read all durable players in one league with optional roster ownership.
    pub fn fantasy_players(
        &self,
        league_key: &str,
    ) -> Result<Vec<StoredFantasyPlayer>, StoreError> {
        validate_identity("read fantasy players", "league key", league_key)?;
        let sql = "SELECT p.yahoo_player_id,p.mlbam_id,p.name,COALESCE(p.mlb_team,''),COALESCE(p.position_type,''),COALESCE(p.eligible_positions,p.display_position,''),CASE WHEN COALESCE(p.status,'') NOT IN ('','IL') THEN p.status WHEN (SELECT r.status FROM mlb_team_active_rosters r WHERE r.mlbam_id=p.mlbam_id ORDER BY CASE WHEN r.primary_type=CASE WHEN p.position_type='P' THEN 'P' ELSE 'H' END THEN 0 ELSE 1 END LIMIT 1)='D7' THEN 'IL7' WHEN (SELECT r.status FROM mlb_team_active_rosters r WHERE r.mlbam_id=p.mlbam_id ORDER BY CASE WHEN r.primary_type=CASE WHEN p.position_type='P' THEN 'P' ELSE 'H' END THEN 0 ELSE 1 END LIMIT 1)='D10' THEN 'IL10' WHEN (SELECT r.status FROM mlb_team_active_rosters r WHERE r.mlbam_id=p.mlbam_id ORDER BY CASE WHEN r.primary_type=CASE WHEN p.position_type='P' THEN 'P' ELSE 'H' END THEN 0 ELSE 1 END LIMIT 1)='D15' THEN 'IL15' WHEN (SELECT r.status FROM mlb_team_active_rosters r WHERE r.mlbam_id=p.mlbam_id ORDER BY CASE WHEN r.primary_type=CASE WHEN p.position_type='P' THEN 'P' ELSE 'H' END THEN 0 ELSE 1 END LIMIT 1)='D60' THEN 'IL60' ELSE COALESCE(p.status,'') END,p.yahoo_rank,p.percent_owned,t.name,ys.slot_position,
COALESCE(h.pa,0),COALESCE(h.obp,0),COALESCE(h.r,0),COALESCE(h.hr,0),COALESCE(h.rbi,0),COALESCE(h.sb,0),COALESCE(h.avg,0),
COALESCE(q.ip,0),COALESCE(q.qs,0),COALESCE(q.w,0),COALESCE(q.sv,0),COALESCE(q.k,0),COALESCE(q.era,0),COALESCE(q.whip,0),COALESCE(p.bat_side,''),COALESCE(NULLIF(p.injury_note,''),p.mlbam_injury_note,''),COALESCE(p.birth_date,''),
sh.xwoba,sh.exit_velo_avg,sh.barrel_pct,sh.hard_hit_pct,sh.strikeout_pct,sh.walk_pct,sh.sprint_speed,sh.ops,
sp.fastball_velo,sp.whiff_pct,sp.chase_pct,sp.gb_pct,sp.strikeout_pct,sp.walk_pct,COALESCE(p.is_closer,0)
FROM players p LEFT JOIN yahoo_roster_slots ys ON ys.player_id=p.id AND ys.slot_position<>'--' AND ys.team_key IN (SELECT team_key FROM yahoo_teams WHERE league_key=?1) LEFT JOIN yahoo_teams t ON t.team_key=ys.team_key
LEFT JOIN yahoo_free_agents fa ON fa.player_id=p.id AND fa.league_key=?1
LEFT JOIN mlbam_season_stats h ON h.player_id=(SELECT hs.player_id FROM mlbam_season_stats hs JOIN players hp ON hp.id=hs.player_id WHERE hp.mlbam_id=p.mlbam_id AND hs.stat_group='hitting' AND hs.season=(SELECT MAX(season) FROM mlbam_season_stats) ORDER BY CASE WHEN hp.mlbam_match_source='seed' THEN 0 ELSE 1 END DESC,hs.synced_at DESC,hs.player_id LIMIT 1) AND h.stat_group='hitting' AND h.season=(SELECT MAX(season) FROM mlbam_season_stats)
LEFT JOIN mlbam_season_stats q ON q.player_id=(SELECT qs.player_id FROM mlbam_season_stats qs JOIN players qp ON qp.id=qs.player_id WHERE qp.mlbam_id=p.mlbam_id AND qs.stat_group='pitching' AND qs.season=(SELECT MAX(season) FROM mlbam_season_stats) ORDER BY CASE WHEN qp.mlbam_match_source='seed' THEN 0 ELSE 1 END DESC,qs.synced_at DESC,qs.player_id LIMIT 1) AND q.stat_group='pitching' AND q.season=(SELECT MAX(season) FROM mlbam_season_stats)
LEFT JOIN statcast_seasons sh ON sh.player_id=(SELECT p2.id FROM players p2 WHERE p2.mlbam_id=p.mlbam_id ORDER BY CASE WHEN p2.mlbam_match_source='seed' THEN 0 ELSE 1 END DESC,p2.yahoo_player_id IS NULL,p2.id LIMIT 1) AND sh.stat_group='batting' AND sh.season=(SELECT MAX(season) FROM statcast_seasons)
LEFT JOIN statcast_seasons sp ON sp.player_id=(SELECT p2.id FROM players p2 WHERE p2.mlbam_id=p.mlbam_id ORDER BY CASE WHEN p2.mlbam_match_source='seed' THEN 0 ELSE 1 END DESC,p2.yahoo_player_id IS NULL,p2.id LIMIT 1) AND sp.stat_group='pitching' AND sp.season=(SELECT MAX(season) FROM statcast_seasons)
WHERE p.yahoo_player_id IS NOT NULL AND (t.team_key IS NOT NULL OR fa.player_id IS NOT NULL) ORDER BY COALESCE(p.yahoo_rank,999999),p.name";
        let mut statement = self
            .connection()
            .prepare(sql)
            .map_err(|error| StoreError::operation("prepare fantasy players", &self.path, error))?;
        let rows = statement
            .query_map([league_key], |row| {
                Ok(StoredFantasyPlayer {
                    yahoo_player_id: row.get(0)?,
                    mlbam_id: row.get(1)?,
                    name: row.get(2)?,
                    team: row.get(3)?,
                    role: row.get(4)?,
                    positions: row.get(5)?,
                    is_closer: row.get(42)?,
                    status: row.get(6)?,
                    injury_note: row.get(26)?,
                    birth_date: row.get(27)?,
                    game_status: String::new(),
                    game_indicator: crate::domain::GameIndicator::None,
                    hand: row.get(25)?,
                    rank: row.get(7)?,
                    percent_owned: row.get(8)?,
                    owner: row
                        .get::<_, Option<String>>(9)?
                        .map(|name| clean_fantasy_team_name(&name)),
                    slot: row.get(10)?,
                    batting: [
                        row.get(11)?,
                        row.get(12)?,
                        row.get(13)?,
                        row.get(14)?,
                        row.get(15)?,
                        row.get(16)?,
                        row.get(17)?,
                    ],
                    pitching: [
                        row.get(18)?,
                        row.get(19)?,
                        row.get(20)?,
                        row.get(21)?,
                        row.get(22)?,
                        row.get(23)?,
                        row.get(24)?,
                    ],
                    hitting_advanced: [
                        row.get(28)?,
                        row.get(29)?,
                        row.get(30)?,
                        row.get(31)?,
                        row.get(32)?,
                        row.get(33)?,
                        row.get(34)?,
                        row.get(35)?,
                    ],
                    pitching_advanced: [
                        row.get(36)?,
                        row.get(37)?,
                        row.get(38)?,
                        row.get(39)?,
                        row.get(40)?,
                        row.get(41)?,
                    ],
                })
            })
            .map_err(|error| StoreError::operation("query fantasy players", &self.path, error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| StoreError::operation("read fantasy players", &self.path, error))
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
