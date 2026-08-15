//! MLB StatsAPI acquisition through b9's transport and cache boundaries.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use serde::Deserialize;

use super::ProviderError;
use crate::cache::{CacheLookup, DiskCache};
use crate::transport::{HttpClient, HttpMethod, HttpRequest};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const RESPONSE_BODY_LIMIT: usize = 8 * 1024 * 1024;
const SCHEDULE_TTL: Duration = Duration::from_secs(60);
const PEOPLE_BATCH_SIZE: usize = 100;
const MAX_ISSUE_DETAIL: usize = 256;

/// Production MLB StatsAPI endpoint configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MlbEndpoints {
    root: Url,
}

impl MlbEndpoints {
    /// Construct a validated MLB endpoint root.
    pub fn new(root: &str) -> Result<Self, ProviderError> {
        let parsed = Url::parse(root).map_err(|error| {
            ProviderError::operation("configure MLB endpoint", "parse endpoint", error)
        })?;
        let loopback_http = parsed.scheme() == "http"
            && parsed
                .host_str()
                .is_some_and(|host| host == "127.0.0.1" || host == "::1" || host == "localhost");
        if parsed.scheme() != "https" && !loopback_http {
            return Err(ProviderError::invalid(
                "configure MLB endpoint",
                "endpoint must use HTTPS except for loopback tests",
            ));
        }
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(ProviderError::invalid(
                "configure MLB endpoint",
                "endpoint must not contain credentials, query, or fragment",
            ));
        }
        Ok(Self { root: parsed })
    }

    /// Return the production MLB endpoint root.
    pub fn production() -> Self {
        Self::new("https://statsapi.mlb.com/api/v1/").expect("static MLB endpoint is valid")
    }
}

/// MLB season boundaries preserved as provider date strings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeasonDates {
    pub season_id: String,
    pub regular_start: String,
    pub regular_end: String,
    pub spring_start: String,
    pub spring_end: String,
}

/// One lineup player in provider order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineupPlayer {
    pub person_id: i64,
    pub full_name: String,
}

/// Hydrated live state for one scheduled game.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Linescore {
    pub inning: Option<i64>,
    pub inning_ordinal: String,
    pub inning_state: String,
    pub away_runs: i64,
    pub home_runs: i64,
}

/// One MLB schedule game.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleGame {
    pub game_id: i64,
    pub game_date: String,
    pub detailed_state: String,
    pub away_team_id: i64,
    pub away_team_name: String,
    pub home_team_id: i64,
    pub home_team_name: String,
    pub away_probable_pitcher_id: Option<i64>,
    pub away_probable_pitcher_name: String,
    pub home_probable_pitcher_id: Option<i64>,
    pub home_probable_pitcher_name: String,
    pub linescore: Option<Linescore>,
    pub away_lineup: Option<Vec<LineupPlayer>>,
    pub home_lineup: Option<Vec<LineupPlayer>>,
}

/// Optional batting values from a boxscore player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoxscoreBatting {
    pub hits: Option<i64>,
    pub at_bats: Option<i64>,
    pub runs: Option<i64>,
    pub home_runs: Option<i64>,
    pub rbi: Option<i64>,
    pub stolen_bases: Option<i64>,
}

/// Optional pitching values from a boxscore player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoxscorePitching {
    pub innings_pitched: Option<String>,
    pub wins: Option<i64>,
    pub saves: Option<i64>,
    pub strikeouts: Option<i64>,
    pub era: Option<String>,
    pub whip: Option<String>,
    pub earned_runs: Option<i64>,
    pub hits_allowed: Option<i64>,
    pub walks: Option<i64>,
}

/// One keyed boxscore player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoxscorePlayer {
    pub person_id: i64,
    pub full_name: String,
    pub batting: Option<BoxscoreBatting>,
    pub pitching: Option<BoxscorePitching>,
}

/// One side of an MLB boxscore.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoxscoreTeam {
    pub batting_order: Vec<i64>,
    pub bench: Vec<i64>,
    pub players: BTreeMap<i64, BoxscorePlayer>,
}

/// Both sides of one MLB boxscore.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Boxscore {
    pub away: BoxscoreTeam,
    pub home: BoxscoreTeam,
}

/// One MLB standings row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamStanding {
    pub team_id: i64,
    pub wins: i64,
    pub losses: i64,
    pub games_back: String,
}

/// Hitter or pitcher compatibility classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimaryType {
    H,
    P,
}

/// One normalized 40-man roster member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RosterMember {
    pub person_id: i64,
    pub full_name: String,
    pub position: String,
    pub primary_type: PrimaryType,
    pub status: String,
    pub jersey_number: String,
}

/// One people endpoint identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonIdentity {
    pub person_id: i64,
    pub full_name: String,
    pub primary_position: String,
    pub bat_side: String,
    pub pitch_hand: String,
    pub current_team: String,
    pub birth_date: Option<String>,
}

/// The cache disposition that led to a schedule result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleCacheStatus {
    Hit,
    Miss,
    Expired,
    Corrupt,
}

/// One cached or live schedule result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachedSchedule {
    pub games: Vec<ScheduleGame>,
    pub cache_status: ScheduleCacheStatus,
    pub cache_write_issue: Option<String>,
}

/// Acquires MLB JSON through injected transport and optional cache calls.
pub struct MlbClient {
    http: Arc<HttpClient>,
    endpoints: MlbEndpoints,
}

impl MlbClient {
    /// Construct an MLB adapter with injected transport and endpoints.
    pub fn new(http: Arc<HttpClient>, endpoints: MlbEndpoints) -> Self {
        Self { http, endpoints }
    }

    /// Construct an MLB adapter with production endpoints.
    pub fn production(http: Arc<HttpClient>) -> Self {
        Self::new(http, MlbEndpoints::production())
    }

    /// Fetch one season's date boundaries.
    pub fn fetch_season_dates(&self, season: i64) -> Result<SeasonDates, ProviderError> {
        validate_season(season)?;
        let url = self.url(&["seasons", &season.to_string()], &[("sportId", "1")]);
        let response: SeasonResponse = self.get_json("fetch MLB season dates", url)?;
        let season = response
            .seasons
            .and_then(|values| values.into_iter().next())
            .ok_or_else(|| {
                ProviderError::invalid(
                    "fetch MLB season dates",
                    "season envelope is absent or empty",
                )
            })?;
        Ok(SeasonDates {
            season_id: season.season_id,
            regular_start: season.regular_start,
            regular_end: season.regular_end,
            spring_start: season.spring_start,
            spring_end: season.spring_end,
        })
    }

    /// Fetch one day's hydrated schedule without disk caching.
    pub fn fetch_schedule(&self, date: &str) -> Result<Vec<ScheduleGame>, ProviderError> {
        validate_date(date)?;
        let (_, games) = self.fetch_schedule_payload(date)?;
        Ok(games)
    }

    /// Fetch one day's schedule through the bounded MLB cache.
    pub fn fetch_schedule_cached(
        &self,
        date: &str,
        cache: &DiskCache,
    ) -> Result<CachedSchedule, ProviderError> {
        validate_date(date)?;
        let key = format!("schedule-{date}");
        let lookup = cache.get("mlb", &key, SCHEDULE_TTL).map_err(|error| {
            ProviderError::operation("fetch cached MLB schedule", "read schedule cache", error)
        })?;
        let status = match lookup {
            CacheLookup::Hit(entry) => match decode_schedule(&entry.payload) {
                Ok(games) => {
                    return Ok(CachedSchedule {
                        games,
                        cache_status: ScheduleCacheStatus::Hit,
                        cache_write_issue: None,
                    });
                }
                Err(_) => ScheduleCacheStatus::Corrupt,
            },
            CacheLookup::Missing => ScheduleCacheStatus::Miss,
            CacheLookup::Expired(entry) => {
                if decode_schedule(&entry.payload).is_err() {
                    ScheduleCacheStatus::Corrupt
                } else {
                    ScheduleCacheStatus::Expired
                }
            }
            CacheLookup::Corrupt { .. } => ScheduleCacheStatus::Corrupt,
        };
        let (payload, games) = self.fetch_schedule_payload(date)?;
        let cache_write_issue = cache
            .put("mlb", &key, &payload)
            .err()
            .map(|error| bounded(&error.to_string(), MAX_ISSUE_DETAIL));
        Ok(CachedSchedule {
            games,
            cache_status: status,
            cache_write_issue,
        })
    }

    /// Fetch one game boxscore.
    pub fn fetch_boxscore(&self, game_id: i64) -> Result<Boxscore, ProviderError> {
        validate_id("game ID", game_id)?;
        let response: BoxscoreResponse = self.get_json(
            "fetch MLB boxscore",
            self.url(&["game", &game_id.to_string(), "boxscore"], &[]),
        )?;
        let teams = response.teams.ok_or_else(|| {
            ProviderError::invalid("fetch MLB boxscore", "teams envelope is absent")
        })?;
        Ok(Boxscore {
            away: convert_boxscore_team(teams.away),
            home: convert_boxscore_team(teams.home),
        })
    }

    /// Fetch AL and NL standings for one season.
    pub fn fetch_standings(&self, season: i64) -> Result<Vec<TeamStanding>, ProviderError> {
        validate_season(season)?;
        let season_value = season.to_string();
        let response: StandingsResponse = self.get_json(
            "fetch MLB standings",
            self.url(
                &["standings"],
                &[("leagueId", "103,104"), ("season", &season_value)],
            ),
        )?;
        let records = response.records.ok_or_else(|| {
            ProviderError::invalid("fetch MLB standings", "records envelope is absent")
        })?;
        Ok(records
            .into_iter()
            .flat_map(|record| record.team_records)
            .filter(|row| row.team.id > 0)
            .map(|row| TeamStanding {
                team_id: row.team.id,
                wins: row.wins,
                losses: row.losses,
                games_back: row.games_back,
            })
            .collect())
    }

    /// Fetch one team's normalized 40-man roster.
    pub fn fetch_roster(&self, team_id: i64) -> Result<Vec<RosterMember>, ProviderError> {
        validate_id("team ID", team_id)?;
        let response: RosterResponse = self.get_json(
            "fetch MLB 40-man roster",
            self.url(
                &["teams", &team_id.to_string(), "roster"],
                &[("rosterType", "40Man")],
            ),
        )?;
        let roster = response.roster.ok_or_else(|| {
            ProviderError::invalid("fetch MLB 40-man roster", "roster envelope is absent")
        })?;
        let mut members = Vec::new();
        for row in roster.into_iter().filter(|row| row.person.id > 0) {
            let position = row.position.abbreviation.trim().to_uppercase();
            let mut status = row.status.code.trim().to_uppercase();
            if status.is_empty() {
                status = "A".into();
            }
            let base = RosterMember {
                person_id: row.person.id,
                full_name: row.person.full_name,
                position: position.clone(),
                primary_type: PrimaryType::H,
                status,
                jersey_number: row.jersey_number.trim().into(),
            };
            if position == "TWP" {
                members.push(base.clone());
                members.push(RosterMember {
                    primary_type: PrimaryType::P,
                    ..base
                });
            } else {
                members.push(RosterMember {
                    primary_type: if matches!(position.as_str(), "P" | "SP" | "RP") {
                        PrimaryType::P
                    } else {
                        PrimaryType::H
                    },
                    ..base
                });
            }
        }
        Ok(members)
    }

    /// Fetch people identities in stable batches of at most 100 IDs.
    pub fn fetch_people(&self, ids: &[i64]) -> Result<Vec<PersonIdentity>, ProviderError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        for id in ids {
            validate_id("person ID", *id)?;
        }
        let mut seen = HashSet::new();
        let unique: Vec<i64> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();
        let requested: HashSet<i64> = unique.iter().copied().collect();
        let mut found = HashMap::new();
        for batch in unique.chunks(PEOPLE_BATCH_SIZE) {
            let joined = batch
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let response: PeopleResponse = self.get_json(
                "fetch MLB people identities",
                self.url(&["people"], &[("personIds", &joined)]),
            )?;
            let people = response.people.ok_or_else(|| {
                ProviderError::invalid("fetch MLB people identities", "people envelope is absent")
            })?;
            for person in people {
                if person.id <= 0 || !requested.contains(&person.id) {
                    continue;
                }
                found.entry(person.id).or_insert_with(|| PersonIdentity {
                    person_id: person.id,
                    full_name: person.full_name,
                    primary_position: person.primary_position.abbreviation.trim().to_uppercase(),
                    bat_side: person.bat_side.code,
                    pitch_hand: person.pitch_hand.code,
                    current_team: person.current_team.abbreviation,
                    birth_date: person.birth_date,
                });
            }
        }
        Ok(unique
            .into_iter()
            .filter_map(|id| found.remove(&id))
            .collect())
    }

    fn fetch_schedule_payload(
        &self,
        date: &str,
    ) -> Result<(Vec<u8>, Vec<ScheduleGame>), ProviderError> {
        let url = self.url(
            &["schedule"],
            &[
                ("sportId", "1"),
                ("date", date),
                ("hydrate", "linescore,probablePitcher,lineups"),
            ],
        );
        let payload = self.get_bytes("fetch MLB schedule", url)?;
        let games = decode_schedule(&payload)?;
        Ok((payload, games))
    }

    fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        operation: &'static str,
        url: Url,
    ) -> Result<T, ProviderError> {
        let payload = self.get_bytes(operation, url)?;
        serde_json::from_slice(&payload)
            .map_err(|error| ProviderError::operation(operation, "decode JSON response", error))
    }

    fn get_bytes(&self, operation: &'static str, url: Url) -> Result<Vec<u8>, ProviderError> {
        let response = self
            .http
            .execute(HttpRequest {
                method: HttpMethod::Get,
                url: url.into(),
                headers: Vec::new(),
                body: Vec::new(),
                timeout: REQUEST_TIMEOUT,
                body_limit: RESPONSE_BODY_LIMIT,
            })
            .map_err(|error| ProviderError::operation(operation, "request failed", error))?;
        if response.status != 200 {
            return Err(ProviderError::invalid(
                operation,
                format!("provider returned HTTP {}", response.status),
            ));
        }
        Ok(response.body)
    }

    fn url(&self, segments: &[&str], query: &[(&str, &str)]) -> Url {
        let mut url = self.endpoints.root.clone();
        {
            let mut path = url
                .path_segments_mut()
                .expect("validated MLB endpoint is hierarchical");
            path.pop_if_empty();
            path.extend(segments.iter().copied());
        }
        url.query_pairs_mut().extend_pairs(query.iter().copied());
        url
    }
}

fn validate_season(season: i64) -> Result<(), ProviderError> {
    if !(1876..=9999).contains(&season) {
        return Err(ProviderError::invalid(
            "validate MLB season",
            "season must be from 1876 through 9999",
        ));
    }
    Ok(())
}

fn validate_id(label: &str, value: i64) -> Result<(), ProviderError> {
    if value <= 0 {
        return Err(ProviderError::invalid(
            "validate MLB identifier",
            format!("{label} must be positive"),
        ));
    }
    Ok(())
}

fn validate_date(value: &str) -> Result<(), ProviderError> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| !matches!(index, 4 | 7) && !byte.is_ascii_digit())
    {
        return Err(invalid_date());
    }
    let year: i64 = value[0..4].parse().map_err(|_| invalid_date())?;
    let month: u32 = value[5..7].parse().map_err(|_| invalid_date())?;
    let day: u32 = value[8..10].parse().map_err(|_| invalid_date())?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if !(1876..=9999).contains(&year) || day == 0 || day > days {
        return Err(invalid_date());
    }
    Ok(())
}

fn invalid_date() -> ProviderError {
    ProviderError::invalid(
        "validate MLB schedule date",
        "date must be a real MLB calendar date in YYYY-MM-DD form",
    )
}

fn decode_schedule(payload: &[u8]) -> Result<Vec<ScheduleGame>, ProviderError> {
    let response: ScheduleResponse = serde_json::from_slice(payload).map_err(|error| {
        ProviderError::operation("fetch MLB schedule", "decode JSON response", error)
    })?;
    let dates = response
        .dates
        .ok_or_else(|| ProviderError::invalid("fetch MLB schedule", "dates envelope is absent"))?;
    Ok(dates
        .into_iter()
        .flat_map(|date| date.games)
        .filter_map(convert_schedule_game)
        .collect())
}

fn convert_schedule_game(game: ScheduleGameWire) -> Option<ScheduleGame> {
    if game.game_id <= 0 || game.teams.away.team.id <= 0 || game.teams.home.team.id <= 0 {
        return None;
    }
    let linescore = game.linescore.map(|line| Linescore {
        inning: line.current_inning,
        inning_ordinal: line.current_inning_ordinal,
        inning_state: line.inning_state,
        away_runs: line.teams.away.runs,
        home_runs: line.teams.home.runs,
    });
    let (away_lineup, home_lineup) = game.lineups.map_or((None, None), |lineups| {
        (
            Some(convert_lineup(lineups.away_players)),
            Some(convert_lineup(lineups.home_players)),
        )
    });
    Some(ScheduleGame {
        game_id: game.game_id,
        game_date: game.game_date,
        detailed_state: game.status.detailed_state,
        away_team_id: game.teams.away.team.id,
        away_team_name: game.teams.away.team.name,
        home_team_id: game.teams.home.team.id,
        home_team_name: game.teams.home.team.name,
        away_probable_pitcher_id: positive(game.teams.away.probable_pitcher.id),
        away_probable_pitcher_name: game.teams.away.probable_pitcher.name,
        home_probable_pitcher_id: positive(game.teams.home.probable_pitcher.id),
        home_probable_pitcher_name: game.teams.home.probable_pitcher.name,
        linescore,
        away_lineup,
        home_lineup,
    })
}

fn convert_lineup(players: Vec<LineupPlayerWire>) -> Vec<LineupPlayer> {
    players
        .into_iter()
        .filter(|player| player.id > 0)
        .map(|player| LineupPlayer {
            person_id: player.id,
            full_name: player.full_name,
        })
        .collect()
}

fn convert_boxscore_team(team: BoxscoreTeamWire) -> BoxscoreTeam {
    let players = team
        .players
        .into_values()
        .filter(|player| player.person.id > 0)
        .map(|player| {
            let id = player.person.id;
            let batting = player.stats.batting.into_public();
            let pitching = player.stats.pitching.into_public();
            (
                id,
                BoxscorePlayer {
                    person_id: id,
                    full_name: player.person.full_name,
                    batting,
                    pitching,
                },
            )
        })
        .collect();
    BoxscoreTeam {
        batting_order: team
            .batting_order
            .into_iter()
            .filter(|id| *id > 0)
            .collect(),
        bench: team.bench.into_iter().filter(|id| *id > 0).collect(),
        players,
    }
}

fn positive(value: i64) -> Option<i64> {
    (value > 0).then_some(value)
}

fn bounded(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[derive(Deserialize)]
struct SeasonResponse {
    seasons: Option<Vec<SeasonWire>>,
}

#[derive(Deserialize)]
struct SeasonWire {
    #[serde(default, rename = "seasonId")]
    season_id: String,
    #[serde(default, rename = "regularSeasonStartDate")]
    regular_start: String,
    #[serde(default, rename = "regularSeasonEndDate")]
    regular_end: String,
    #[serde(default, rename = "preSeasonStartDate")]
    spring_start: String,
    #[serde(default, rename = "preSeasonEndDate")]
    spring_end: String,
}

#[derive(Deserialize)]
struct ScheduleResponse {
    dates: Option<Vec<ScheduleDateWire>>,
}

#[derive(Deserialize)]
struct ScheduleDateWire {
    #[serde(default)]
    games: Vec<ScheduleGameWire>,
}

#[derive(Deserialize)]
struct ScheduleGameWire {
    #[serde(default, rename = "gamePk")]
    game_id: i64,
    #[serde(default, rename = "gameDate")]
    game_date: String,
    #[serde(default)]
    status: StatusWire,
    #[serde(default)]
    teams: ScheduleTeamsWire,
    linescore: Option<LinescoreWire>,
    lineups: Option<LineupsWire>,
}

#[derive(Default, Deserialize)]
struct StatusWire {
    #[serde(default, rename = "detailedState")]
    detailed_state: String,
}

#[derive(Default, Deserialize)]
struct ScheduleTeamsWire {
    #[serde(default)]
    away: ScheduleSideWire,
    #[serde(default)]
    home: ScheduleSideWire,
}

#[derive(Default, Deserialize)]
struct ScheduleSideWire {
    #[serde(default)]
    team: NamedIdWire,
    #[serde(default, rename = "probablePitcher")]
    probable_pitcher: NamedIdWire,
}

#[derive(Default, Deserialize)]
struct NamedIdWire {
    #[serde(default)]
    id: i64,
    #[serde(default, rename = "name", alias = "fullName")]
    name: String,
}

#[derive(Deserialize)]
struct LinescoreWire {
    #[serde(rename = "currentInning")]
    current_inning: Option<i64>,
    #[serde(default, rename = "currentInningOrdinal")]
    current_inning_ordinal: String,
    #[serde(default, rename = "inningState")]
    inning_state: String,
    #[serde(default)]
    teams: LinescoreTeamsWire,
}

#[derive(Default, Deserialize)]
struct LinescoreTeamsWire {
    #[serde(default)]
    away: RunsWire,
    #[serde(default)]
    home: RunsWire,
}

#[derive(Default, Deserialize)]
struct RunsWire {
    #[serde(default)]
    runs: i64,
}

#[derive(Deserialize)]
struct LineupsWire {
    #[serde(default, rename = "awayPlayers")]
    away_players: Vec<LineupPlayerWire>,
    #[serde(default, rename = "homePlayers")]
    home_players: Vec<LineupPlayerWire>,
}

#[derive(Deserialize)]
struct LineupPlayerWire {
    #[serde(default)]
    id: i64,
    #[serde(default, rename = "fullName")]
    full_name: String,
}

#[derive(Deserialize)]
struct BoxscoreResponse {
    teams: Option<BoxscoreTeamsWire>,
}

#[derive(Deserialize)]
struct BoxscoreTeamsWire {
    away: BoxscoreTeamWire,
    home: BoxscoreTeamWire,
}

#[derive(Deserialize)]
struct BoxscoreTeamWire {
    #[serde(default, rename = "battingOrder")]
    batting_order: Vec<i64>,
    #[serde(default)]
    bench: Vec<i64>,
    #[serde(default)]
    players: HashMap<String, BoxscorePlayerWire>,
}

#[derive(Deserialize)]
struct BoxscorePlayerWire {
    #[serde(default)]
    person: PersonWire,
    #[serde(default)]
    stats: BoxscoreStatsWire,
}

#[derive(Default, Deserialize)]
struct BoxscoreStatsWire {
    #[serde(default)]
    batting: BoxscoreBattingWire,
    #[serde(default)]
    pitching: BoxscorePitchingWire,
}

#[derive(Default, Deserialize)]
struct BoxscoreBattingWire {
    hits: Option<i64>,
    #[serde(rename = "atBats")]
    at_bats: Option<i64>,
    runs: Option<i64>,
    #[serde(rename = "homeRuns")]
    home_runs: Option<i64>,
    rbi: Option<i64>,
    #[serde(rename = "stolenBases")]
    stolen_bases: Option<i64>,
}

impl BoxscoreBattingWire {
    fn into_public(self) -> Option<BoxscoreBatting> {
        let present = self.hits.is_some()
            || self.at_bats.is_some()
            || self.runs.is_some()
            || self.home_runs.is_some()
            || self.rbi.is_some()
            || self.stolen_bases.is_some();
        present.then_some(BoxscoreBatting {
            hits: self.hits,
            at_bats: self.at_bats,
            runs: self.runs,
            home_runs: self.home_runs,
            rbi: self.rbi,
            stolen_bases: self.stolen_bases,
        })
    }
}

#[derive(Default, Deserialize)]
struct BoxscorePitchingWire {
    #[serde(rename = "inningsPitched")]
    innings_pitched: Option<String>,
    wins: Option<i64>,
    saves: Option<i64>,
    #[serde(rename = "strikeOuts")]
    strikeouts: Option<i64>,
    era: Option<String>,
    whip: Option<String>,
    #[serde(rename = "earnedRuns")]
    earned_runs: Option<i64>,
    hits: Option<i64>,
    #[serde(rename = "baseOnBalls")]
    walks: Option<i64>,
}

impl BoxscorePitchingWire {
    fn into_public(self) -> Option<BoxscorePitching> {
        let present = self.innings_pitched.is_some()
            || self.wins.is_some()
            || self.saves.is_some()
            || self.strikeouts.is_some()
            || self.era.is_some()
            || self.whip.is_some()
            || self.earned_runs.is_some()
            || self.hits.is_some()
            || self.walks.is_some();
        present.then_some(BoxscorePitching {
            innings_pitched: self.innings_pitched,
            wins: self.wins,
            saves: self.saves,
            strikeouts: self.strikeouts,
            era: self.era,
            whip: self.whip,
            earned_runs: self.earned_runs,
            hits_allowed: self.hits,
            walks: self.walks,
        })
    }
}

#[derive(Deserialize)]
struct StandingsResponse {
    records: Option<Vec<StandingRecordWire>>,
}

#[derive(Deserialize)]
struct StandingRecordWire {
    #[serde(default, rename = "teamRecords")]
    team_records: Vec<TeamStandingWire>,
}

#[derive(Deserialize)]
struct TeamStandingWire {
    #[serde(default)]
    team: IdWire,
    #[serde(default)]
    wins: i64,
    #[serde(default)]
    losses: i64,
    #[serde(default, rename = "gamesBack")]
    games_back: String,
}

#[derive(Default, Deserialize)]
struct IdWire {
    #[serde(default)]
    id: i64,
}

#[derive(Deserialize)]
struct RosterResponse {
    roster: Option<Vec<RosterMemberWire>>,
}

#[derive(Deserialize)]
struct RosterMemberWire {
    #[serde(default)]
    person: PersonWire,
    #[serde(default)]
    position: AbbreviationWire,
    #[serde(default)]
    status: CodeWire,
    #[serde(default, rename = "jerseyNumber")]
    jersey_number: String,
}

#[derive(Default, Deserialize)]
struct PersonWire {
    #[serde(default)]
    id: i64,
    #[serde(default, rename = "fullName")]
    full_name: String,
}

#[derive(Default, Deserialize)]
struct AbbreviationWire {
    #[serde(default)]
    abbreviation: String,
}

#[derive(Default, Deserialize)]
struct CodeWire {
    #[serde(default)]
    code: String,
}

#[derive(Deserialize)]
struct PeopleResponse {
    people: Option<Vec<PersonIdentityWire>>,
}

#[derive(Deserialize)]
struct PersonIdentityWire {
    #[serde(default)]
    id: i64,
    #[serde(default, rename = "fullName")]
    full_name: String,
    #[serde(default, rename = "primaryPosition")]
    primary_position: AbbreviationWire,
    #[serde(default, rename = "batSide")]
    bat_side: CodeWire,
    #[serde(default, rename = "pitchHand")]
    pitch_hand: CodeWire,
    #[serde(default, rename = "currentTeam")]
    current_team: AbbreviationWire,
    #[serde(rename = "birthDate")]
    birth_date: Option<String>,
}
