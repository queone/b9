//! Typed Yahoo Fantasy acquisition and numeric-key JSON normalization.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::domain::{
    FantasyPlayer, FantasyRosterSlot, FantasyTeam, League, Matchup, MatchupTeam, PlayerWeekStats,
    Position, RosterWeekStats, ScoringType, clean_fantasy_team_name,
};

use super::yahoo::{YahooClient, YahooError};

const MAX_PAGES: usize = 20;

/// One league visible to the authenticated Yahoo user.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserLeague {
    pub league_key: String,
    pub name: String,
    pub season: i32,
    pub team_key: String,
    pub team_name: String,
}

/// One scoring category normalized from Yahoo settings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatCategory {
    pub stat_id: i64,
    pub abbreviation: String,
    pub name: String,
    pub sort_order: i32,
    pub display_only: bool,
    pub sequence: i64,
}

/// One roster position and its league count.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RosterPosition {
    pub position: Position,
    pub count: i64,
}

/// League metadata and scoring settings acquired as one complete payload.
#[derive(Clone, Debug, PartialEq)]
pub struct LeagueSettings {
    pub league: League,
    pub current_week: Option<i32>,
    pub categories: Vec<StatCategory>,
    pub roster_positions: Vec<RosterPosition>,
}

/// One complete normalized league roster response.
#[derive(Clone, Debug, PartialEq)]
pub struct LeagueRosters {
    pub players: Vec<FantasyPlayer>,
    pub slots: Vec<FantasyRosterSlot>,
}

/// One Yahoo fantasy data failure.
#[derive(Debug)]
pub enum YahooFantasyError {
    Yahoo(YahooError),
    InvalidInput(&'static str),
    InvalidPayload(&'static str),
    Incomplete(&'static str),
}

impl fmt::Display for YahooFantasyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Yahoo(error) => write!(formatter, "acquire Yahoo fantasy data: {error}"),
            Self::InvalidInput(detail) => write!(
                formatter,
                "construct Yahoo fantasy request: {detail}; correct the value and retry"
            ),
            Self::InvalidPayload(detail) => write!(
                formatter,
                "parse Yahoo fantasy response: {detail}; retry after Yahoo returns a valid response"
            ),
            Self::Incomplete(detail) => write!(
                formatter,
                "validate Yahoo fantasy snapshot: {detail}; prior complete data was retained"
            ),
        }
    }
}

impl std::error::Error for YahooFantasyError {}

impl YahooFantasyError {
    /// Report whether this failure means authenticated access cannot continue.
    pub fn is_terminal_access(&self) -> bool {
        matches!(self, Self::Yahoo(error) if error.is_terminal_access())
    }
}

impl From<YahooError> for YahooFantasyError {
    fn from(value: YahooError) -> Self {
        Self::Yahoo(value)
    }
}

/// Injectable Yahoo fantasy operations consumed by application layers.
pub trait YahooFantasySource {
    fn user_leagues(&self) -> Result<Vec<UserLeague>, YahooFantasyError>;
    fn team_key(&self, league_key: &str) -> Result<String, YahooFantasyError>;
    fn league_settings(&self, league_key: &str) -> Result<LeagueSettings, YahooFantasyError>;
    fn standings(&self, league_key: &str) -> Result<Vec<FantasyTeam>, YahooFantasyError>;
    fn league_rosters(&self, league_key: &str) -> Result<LeagueRosters, YahooFantasyError>;
    fn free_agents(&self, league_key: &str) -> Result<Vec<FantasyPlayer>, YahooFantasyError>;
    fn scoreboard(
        &self,
        league_key: &str,
        week: Option<i32>,
    ) -> Result<Vec<Matchup>, YahooFantasyError>;
    fn roster_week_stats(
        &self,
        team_key: &str,
        week: i32,
    ) -> Result<RosterWeekStats, YahooFantasyError>;
}

/// Production Yahoo fantasy client layered over authenticated raw requests.
pub struct YahooFantasyClient {
    yahoo: Arc<YahooClient>,
}

impl YahooFantasyClient {
    /// Construct a typed Yahoo fantasy adapter.
    pub fn new(yahoo: Arc<YahooClient>) -> Self {
        Self { yahoo }
    }

    fn get(&self, path: &str) -> Result<Value, YahooFantasyError> {
        let response = self.yahoo.get_raw(path)?;
        serde_json::from_slice(&response.body)
            .map_err(|_| YahooFantasyError::InvalidPayload("response is not valid JSON"))
    }
}

impl YahooFantasySource for YahooFantasyClient {
    fn user_leagues(&self) -> Result<Vec<UserLeague>, YahooFantasyError> {
        parse_user_leagues(&self.get("/users;use_login=1/games;game_keys=mlb/leagues/teams")?)
    }

    fn team_key(&self, league_key: &str) -> Result<String, YahooFantasyError> {
        validate_key(league_key)?;
        self.user_leagues()?
            .into_iter()
            .find(|league| league.league_key == league_key)
            .map(|league| league.team_key)
            .filter(|key| !key.is_empty())
            .ok_or(YahooFantasyError::Incomplete(
                "authenticated team is missing from the selected league",
            ))
    }

    fn league_settings(&self, league_key: &str) -> Result<LeagueSettings, YahooFantasyError> {
        validate_key(league_key)?;
        parse_league_settings(
            league_key,
            &self.get(&format!("/league/{league_key}/settings"))?,
        )
    }

    fn standings(&self, league_key: &str) -> Result<Vec<FantasyTeam>, YahooFantasyError> {
        validate_key(league_key)?;
        parse_standings(
            league_key,
            &self.get(&format!("/league/{league_key}/standings"))?,
        )
    }

    fn league_rosters(&self, league_key: &str) -> Result<LeagueRosters, YahooFantasyError> {
        validate_key(league_key)?;
        parse_league_rosters(
            league_key,
            &self.get(&format!(
                "/league/{league_key}/teams/roster/players;out=ranks,percent_owned"
            ))?,
        )
    }

    fn free_agents(&self, league_key: &str) -> Result<Vec<FantasyPlayer>, YahooFantasyError> {
        validate_key(league_key)?;
        let mut players = BTreeMap::new();
        for start in 0..MAX_PAGES {
            let offset = start * 25;
            let page = parse_free_agents(&self.get(&format!(
                "/league/{league_key}/players;status=A;start={offset};count=25;out=ranks,percent_owned"
            ))?)?;
            if page.is_empty() {
                break;
            }
            for player in page {
                players.insert(player.yahoo_player_id, player);
            }
        }
        (!players.is_empty())
            .then(|| players.into_values().collect())
            .ok_or(YahooFantasyError::Incomplete(
                "free-agent pages contain no players",
            ))
    }

    fn scoreboard(
        &self,
        league_key: &str,
        week: Option<i32>,
    ) -> Result<Vec<Matchup>, YahooFantasyError> {
        validate_key(league_key)?;
        if week.is_some_and(|value| value <= 0) {
            return Err(YahooFantasyError::InvalidInput("week must be positive"));
        }
        let suffix = week.map_or_else(String::new, |value| format!(";week={value}"));
        parse_scoreboard(&self.get(&format!("/league/{league_key}/scoreboard{suffix}"))?)
    }

    fn roster_week_stats(
        &self,
        team_key: &str,
        week: i32,
    ) -> Result<RosterWeekStats, YahooFantasyError> {
        validate_key(team_key)?;
        if week <= 0 {
            return Err(YahooFantasyError::InvalidInput("week must be positive"));
        }
        parse_roster_week_stats(
            team_key,
            week,
            &self.get(&format!(
                "/team/{team_key}/roster;week={week}/players/stats;type=week;week={week}"
            ))?,
        )
    }
}

fn validate_key(key: &str) -> Result<(), YahooFantasyError> {
    if key.trim().is_empty() || key.contains('/') || key.contains('\\') {
        return Err(YahooFantasyError::InvalidInput("provider key is invalid"));
    }
    Ok(())
}

fn parsed_root(value: &Value) -> Result<&Value, YahooFantasyError> {
    value
        .get("fantasy_content")
        .or_else(|| value.get("data"))
        .or(Some(value))
        .ok_or(YahooFantasyError::InvalidPayload(
            "fantasy_content is missing",
        ))
}

fn flattened(value: &Value) -> Map<String, Value> {
    let mut result = Map::new();
    flatten_into(value, &mut result);
    result
}

fn flatten_into(value: &Value, output: &mut Map<String, Value>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if !key.chars().all(|character| character.is_ascii_digit()) {
                    output.entry(key.clone()).or_insert_with(|| value.clone());
                }
                if matches!(value, Value::Array(_) | Value::Object(_)) {
                    flatten_into(value, output);
                }
            }
        }
        Value::Array(values) => values.iter().for_each(|value| flatten_into(value, output)),
        _ => {}
    }
}

fn entity_maps(value: &Value, identity: &str) -> Vec<Map<String, Value>> {
    fn visit(value: &Value, identity: &str, output: &mut Vec<Map<String, Value>>) {
        match value {
            Value::Array(values) => {
                for value in values {
                    visit(value, identity, output);
                }
            }
            Value::Object(values) if values.contains_key(identity) => output.push(flattened(value)),
            Value::Object(values) => {
                values
                    .values()
                    .for_each(|value| visit(value, identity, output));
            }
            _ => {}
        }
    }
    let mut output = Vec::new();
    visit(value, identity, &mut output);
    output
}

fn text(map: &Map<String, Value>, key: &str) -> String {
    map.get(key)
        .map(|value| match value {
            Value::String(value) => value.clone(),
            Value::Number(value) => value.to_string(),
            Value::Bool(value) => value.to_string(),
            _ => String::new(),
        })
        .unwrap_or_default()
}

fn integer(map: &Map<String, Value>, key: &str) -> i64 {
    map.get(key)
        .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
        .unwrap_or(0)
}

fn decimal(map: &Map<String, Value>, key: &str) -> Option<f64> {
    map.get(key)
        .and_then(|value| value.as_f64().or_else(|| value.as_str()?.parse().ok()))
}

fn yahoo_rank(map: &Map<String, Value>) -> Option<i64> {
    fn collect(value: &Value, overall: &mut i64, seasons: &mut BTreeMap<i32, i64>) {
        match value {
            Value::Object(values) => {
                if let Some(player_rank) = values.get("player_rank") {
                    let rank = flattened(player_rank);
                    let value = integer(&rank, "rank_value");
                    if value > 0 {
                        match text(&rank, "rank_type").as_str() {
                            "OR" => *overall = value,
                            "S" => {
                                if let Ok(season) = text(&rank, "rank_season").parse() {
                                    seasons.insert(season, value);
                                }
                            }
                            _ => {}
                        }
                    }
                } else {
                    values
                        .values()
                        .for_each(|value| collect(value, overall, seasons));
                }
            }
            Value::Array(values) => values
                .iter()
                .for_each(|value| collect(value, overall, seasons)),
            _ => {}
        }
    }

    let mut overall = 0;
    let mut seasons = BTreeMap::new();
    if let Some(ranks) = map.get("player_ranks") {
        collect(ranks, &mut overall, &mut seasons);
    }
    let current = seasons.last_key_value().map(|(_, rank)| *rank).unwrap_or(0);
    let previous = seasons
        .iter()
        .rev()
        .nth(1)
        .map(|(_, rank)| *rank)
        .unwrap_or(0);
    let selected = if current > 0 && overall > 0 && current != overall {
        current
    } else if previous > 0 {
        previous
    } else if overall > 0 {
        overall
    } else if current > 0 {
        current
    } else {
        integer(map, "rank_value")
    };
    Some(selected).filter(|rank| *rank > 0)
}

/// Parse authenticated user league discovery JSON.
pub fn parse_user_leagues(value: &Value) -> Result<Vec<UserLeague>, YahooFantasyError> {
    let mut unique = BTreeMap::new();
    for map in entity_maps(parsed_root(value)?, "league_key") {
        let league_key = text(&map, "league_key");
        if league_key.is_empty() {
            continue;
        }
        unique.entry(league_key.clone()).or_insert(UserLeague {
            league_key,
            name: text(&map, "name"),
            season: integer(&map, "season") as i32,
            team_key: text(&map, "team_key"),
            team_name: clean_fantasy_team_name(&text(&map, "team_name")),
        });
    }
    Ok(unique.into_values().collect())
}

/// Parse one league settings response.
pub fn parse_league_settings(
    league_key: &str,
    value: &Value,
) -> Result<LeagueSettings, YahooFantasyError> {
    let root = parsed_root(value)?;
    let map = entity_maps(root, "league_key")
        .into_iter()
        .next()
        .unwrap_or_else(|| flattened(root));
    let name = text(&map, "name");
    let season = integer(&map, "season") as i32;
    let num_teams = integer(&map, "num_teams") as i32;
    if name.is_empty() || season <= 0 || num_teams <= 0 {
        return Err(YahooFantasyError::Incomplete(
            "league metadata is incomplete",
        ));
    }
    let categories = entity_maps(root, "stat_id")
        .into_iter()
        .enumerate()
        .filter_map(|(sequence, map)| {
            let stat_id = integer(&map, "stat_id");
            (stat_id > 0).then(|| StatCategory {
                stat_id,
                abbreviation: text(&map, "abbr"),
                name: text(&map, "name"),
                sort_order: if text(&map, "sort_order") == "0" {
                    0
                } else {
                    1
                },
                display_only: text(&map, "is_only_display_stat") == "1",
                sequence: sequence as i64,
            })
        })
        .collect::<Vec<_>>();
    let roster_positions = entity_maps(root, "position")
        .into_iter()
        .filter_map(|map| {
            let position = text(&map, "position");
            let count = integer(&map, "count");
            (!position.is_empty() && count > 0).then(|| RosterPosition {
                position: Position::from(position),
                count,
            })
        })
        .collect::<Vec<_>>();
    if categories.is_empty() || roster_positions.is_empty() {
        return Err(YahooFantasyError::Incomplete(
            "scoring categories or roster positions are missing",
        ));
    }
    Ok(LeagueSettings {
        current_week: Some(integer(&map, "current_week") as i32).filter(|week| *week > 0),
        league: League {
            league_key: league_key.to_owned(),
            name,
            season,
            num_teams,
            scoring_type: ScoringType::from(text(&map, "scoring_type")),
            roster_positions: roster_positions
                .iter()
                .map(|row| row.position.clone())
                .collect(),
            batting_categories: categories
                .iter()
                .filter(|row| row.stat_id < 50)
                .map(|row| row.abbreviation.clone())
                .collect(),
            pitching_categories: categories
                .iter()
                .filter(|row| row.stat_id >= 50)
                .map(|row| row.abbreviation.clone())
                .collect(),
        },
        categories,
        roster_positions,
    })
}

/// Parse one complete standings response.
pub fn parse_standings(
    league_key: &str,
    value: &Value,
) -> Result<Vec<FantasyTeam>, YahooFantasyError> {
    let mut teams = BTreeMap::new();
    for map in entity_maps(parsed_root(value)?, "team_key") {
        let team_key = text(&map, "team_key");
        if team_key.is_empty() {
            continue;
        }
        teams.entry(team_key.clone()).or_insert(FantasyTeam {
            team_key,
            league_key: league_key.to_owned(),
            team_id: integer(&map, "team_id"),
            name: clean_fantasy_team_name(&text(&map, "name")),
            manager_name: text(&map, "nickname"),
            is_owned_by_current_login: text(&map, "is_owned_by_current_login") == "1",
            waiver_priority: integer(&map, "waiver_priority"),
            faab_balance: integer(&map, "faab_balance"),
            wins: integer(&map, "wins"),
            losses: integer(&map, "losses"),
            ties: integer(&map, "ties"),
            moves: integer(&map, "number_of_moves").max(integer(&map, "moves")),
            rank: integer(&map, "rank"),
        });
    }
    if teams.is_empty() {
        return Err(YahooFantasyError::Incomplete("standings contain no teams"));
    }
    Ok(teams.into_values().collect())
}

/// Parse one complete league-roster response.
pub fn parse_league_rosters(
    _league_key: &str,
    value: &Value,
) -> Result<LeagueRosters, YahooFantasyError> {
    let root = parsed_root(value)?;
    let team_maps = entity_maps(root, "team_key");
    let mut players = BTreeMap::new();
    let mut slots = BTreeSet::new();
    for team in team_maps {
        let team_key = text(&team, "team_key");
        if team_key.is_empty() {
            continue;
        }
        let source = team.get("players").unwrap_or(&Value::Null);
        for map in entity_maps(source, "player_id") {
            let player_id = integer(&map, "player_id");
            let selected = text(&map, "position");
            if player_id <= 0 || selected == "--" {
                continue;
            }
            let eligible = map
                .get("eligible_positions")
                .map(flattened)
                .map(|map| {
                    map.values()
                        .filter_map(Value::as_str)
                        .map(Position::from)
                        .collect()
                })
                .unwrap_or_default();
            players.entry(player_id).or_insert(FantasyPlayer {
                yahoo_player_id: player_id,
                name: text(&map, "full"),
                mlb_team: text(&map, "editorial_team_abbr"),
                display_position: text(&map, "display_position"),
                position_type: text(&map, "position_type"),
                eligible_positions: eligible,
                injury_status: text(&map, "status"),
                percent_owned: decimal(&map, "value"),
                yahoo_rank: yahoo_rank(&map),
            });
            slots.insert((team_key.clone(), player_id, selected));
        }
    }
    if players.is_empty() || slots.is_empty() {
        return Err(YahooFantasyError::Incomplete(
            "complete league roster contains no players or slots",
        ));
    }
    Ok(LeagueRosters {
        players: players.into_values().collect(),
        slots: slots
            .into_iter()
            .map(|(team_key, yahoo_player_id, position)| FantasyRosterSlot {
                team_key,
                yahoo_player_id,
                slot_position: Position::from(position),
            })
            .collect(),
    })
}

/// Parse one free-agent player page.
pub fn parse_free_agents(value: &Value) -> Result<Vec<FantasyPlayer>, YahooFantasyError> {
    let mut players = BTreeMap::new();
    for map in entity_maps(parsed_root(value)?, "player_id") {
        let player_id = integer(&map, "player_id");
        if player_id <= 0 {
            continue;
        }
        let eligible = map
            .get("eligible_positions")
            .map(flattened)
            .map(|map| {
                map.values()
                    .filter_map(Value::as_str)
                    .map(Position::from)
                    .collect()
            })
            .unwrap_or_default();
        players.insert(
            player_id,
            FantasyPlayer {
                yahoo_player_id: player_id,
                name: text(&map, "full"),
                mlb_team: text(&map, "editorial_team_abbr"),
                display_position: text(&map, "display_position"),
                position_type: text(&map, "position_type"),
                eligible_positions: eligible,
                injury_status: text(&map, "status"),
                percent_owned: decimal(&map, "value"),
                yahoo_rank: yahoo_rank(&map),
            },
        );
    }
    Ok(players.into_values().collect())
}

/// Parse one weekly scoreboard response.
pub fn parse_scoreboard(value: &Value) -> Result<Vec<Matchup>, YahooFantasyError> {
    let root = parsed_root(value)?;
    let mut output = Vec::new();
    for map in scoreboard_matchup_maps(root) {
        let week = integer(&map, "week") as i32;
        let team_maps = map
            .get("teams")
            .map(scoreboard_team_maps)
            .unwrap_or_default();
        if week <= 0 || team_maps.len() != 2 {
            continue;
        }
        let mut teams = team_maps
            .into_iter()
            .map(|team| MatchupTeam {
                team_key: text(&team, "team_key"),
                team_id: integer(&team, "team_id"),
                name: clean_fantasy_team_name(&text(&team, "name")),
                is_current_login: text(&team, "is_owned_by_current_login") == "1",
                stats: team_statistics(&team),
                wins: integer(&team, "wins") as i32,
                losses: integer(&team, "losses") as i32,
                ties: integer(&team, "ties") as i32,
                completed_games: integer(&team, "completed_games") as i32,
                live_games: integer(&team, "live_games") as i32,
                remaining_games: integer(&team, "remaining_games") as i32,
            })
            .collect::<Vec<_>>();
        if teams
            .iter()
            .all(|team| team.wins == 0 && team.losses == 0 && team.ties == 0)
        {
            let (wins, losses, ties) = matchup_category_record(&teams[0], &teams[1]);
            teams[0].wins = wins;
            teams[0].losses = losses;
            teams[0].ties = ties;
            teams[1].wins = losses;
            teams[1].losses = wins;
            teams[1].ties = ties;
        }
        output.push(Matchup {
            week,
            week_start: text(&map, "week_start"),
            week_end: text(&map, "week_end"),
            status: text(&map, "status"),
            teams: [teams[0].clone(), teams[1].clone()],
        });
    }
    Ok(output)
}

fn matchup_category_record(mine: &MatchupTeam, opponent: &MatchupTeam) -> (i32, i32, i32) {
    const CATEGORIES: [(&str, bool); 10] = [
        ("7", false),
        ("12", false),
        ("13", false),
        ("16", false),
        ("3", false),
        ("28", false),
        ("32", false),
        ("42", false),
        ("26", true),
        ("27", true),
    ];
    let mut record = (0, 0, 0);
    for (id, lower_wins) in CATEGORIES {
        let values = mine
            .stats
            .get(id)
            .and_then(|value| value.parse::<f64>().ok())
            .zip(
                opponent
                    .stats
                    .get(id)
                    .and_then(|value| value.parse::<f64>().ok()),
            );
        match values {
            Some((mine, opponent)) if mine != opponent => {
                if (mine > opponent) != lower_wins {
                    record.0 += 1;
                } else {
                    record.1 += 1;
                }
            }
            _ => record.2 += 1,
        }
    }
    record
}

fn scoreboard_team_maps(value: &Value) -> Vec<Map<String, Value>> {
    fn visit(value: &Value, output: &mut Vec<Map<String, Value>>) {
        match value {
            Value::Array(values) => values.iter().for_each(|value| visit(value, output)),
            Value::Object(values) => {
                if let Some(team) = values.get("team") {
                    let map = flattened(team);
                    if !text(&map, "team_key").is_empty() {
                        output.push(map);
                        return;
                    }
                }
                values.values().for_each(|value| visit(value, output));
            }
            _ => {}
        }
    }

    let mut output = Vec::new();
    visit(value, &mut output);
    if output.is_empty() {
        entity_maps(value, "team_key")
    } else {
        output
    }
}

fn scoreboard_matchup_maps(value: &Value) -> Vec<Map<String, Value>> {
    fn visit(value: &Value, output: &mut Vec<Map<String, Value>>) {
        match value {
            Value::Array(values) => values.iter().for_each(|value| visit(value, output)),
            Value::Object(values) => {
                if values.contains_key("week")
                    && values.contains_key("week_start")
                    && values.contains_key("week_end")
                {
                    output.push(flattened(value));
                }
                values.values().for_each(|value| visit(value, output));
            }
            _ => {}
        }
    }

    let mut output = Vec::new();
    visit(value, &mut output);
    output
}

fn team_statistics(team: &Map<String, Value>) -> std::collections::HashMap<String, String> {
    team.get("team_stats")
        .or_else(|| team.get("stats"))
        .map(|value| {
            entity_maps(value, "stat_id")
                .into_iter()
                .filter_map(|stat| {
                    let id = text(&stat, "stat_id");
                    let value = text(&stat, "value");
                    (!id.is_empty() && !value.is_empty()).then_some((id, value))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse one team's weekly roster statistics.
pub fn parse_roster_week_stats(
    team_key: &str,
    week: i32,
    value: &Value,
) -> Result<RosterWeekStats, YahooFantasyError> {
    let root = parsed_root(value)?;
    let team = entity_maps(root, "team_key")
        .into_iter()
        .find(|map| text(map, "team_key") == team_key)
        .unwrap_or_else(|| flattened(root));
    let players = entity_maps(team.get("players").unwrap_or(root), "player_id")
        .into_iter()
        .filter_map(|map| {
            let id = integer(&map, "player_id");
            (id > 0).then(|| PlayerWeekStats {
                yahoo_player_id: id,
                name: text(&map, "full"),
                team: text(&map, "editorial_team_abbr"),
                position_type: text(&map, "position_type"),
                slot_position: Position::from(text(&map, "position")),
                eligible_positions: Vec::new(),
                injury_status: text(&map, "status"),
                hab: text(&map, "H/AB"),
                runs: integer(&map, "R") as i32,
                home_runs: integer(&map, "HR") as i32,
                runs_batted_in: integer(&map, "RBI") as i32,
                stolen_bases: integer(&map, "SB") as i32,
                batting_average: text(&map, "AVG"),
                innings_pitched: text(&map, "IP"),
                wins: integer(&map, "W") as i32,
                saves: integer(&map, "SV") as i32,
                strikeouts: integer(&map, "K") as i32,
                earned_run_average: text(&map, "ERA"),
                whip: text(&map, "WHIP"),
            })
        })
        .collect::<Vec<_>>();
    if players.is_empty() {
        return Err(YahooFantasyError::Incomplete(
            "weekly roster contains no players",
        ));
    }
    Ok(RosterWeekStats {
        team_key: team_key.to_owned(),
        team_name: clean_fantasy_team_name(&text(&team, "name")),
        week,
        players,
    })
}

/// Return deterministic page offsets while enforcing the provider-call budget.
pub fn bounded_page_starts(
    total: usize,
    page_size: usize,
) -> Result<Vec<usize>, YahooFantasyError> {
    if page_size == 0 {
        return Err(YahooFantasyError::InvalidInput(
            "page size must be positive",
        ));
    }
    let pages = total.div_ceil(page_size);
    if pages > MAX_PAGES {
        return Err(YahooFantasyError::Incomplete(
            "pagination exceeds the bounded page limit",
        ));
    }
    Ok((0..pages).map(|page| page * page_size).collect())
}
