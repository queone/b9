//! Unauthenticated client for Yahoo's public fantasy redzone feed.
//!
//! `pub-api.fantasysports.yahoo.com/fantasy/v3/redzone/mlb` returns real
//! league data with zero cookies and zero auth headers — confirmed distinct
//! from account-scoped paths on the same host, which return 401 without
//! login. This module owns that wire format; it never touches OAuth, the
//! credential store, or `b9 login` state.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::time::Duration;

use serde::Deserialize;

use crate::domain::{
    FantasyPlayer, FantasyRosterSlot, FantasyTeam, League, Matchup, MatchupTeam, PlayerWeekStats,
    Position, RosterWeekStats, ScoringType,
};
use crate::providers::yahoo_fantasy::RosterPosition;
use crate::transport::{HttpClient, HttpHeader, HttpMethod, HttpRequest};

/// Yahoo stat ids observed to be counting stats — safe to sum directly
/// across a roster. Confirmed against real data, including two ids (AB,
/// batting H) that never appear in a league's own scoring-category list but
/// are present in every player's raw `stats` regardless, since they're the
/// building blocks for the AVG rate stat.
const COUNTING_STAT_IDS: &[&str] = &[
    "1", "7", "12", "13", "16", "6", "8", "28", "32", "42", "33", "37", "39", "34",
];
/// Rate stats — computed from summed counting stats, never summed or
/// averaged directly. `(id, formula-input ids)`.
const AVG_ID: &str = "3";
const AB_ID: &str = "6";
const BATTING_H_ID: &str = "8";
const ERA_ID: &str = "26";
const WHIP_ID: &str = "27";
const OUT_ID: &str = "33";
const EARNED_RUNS_ID: &str = "37";
const PITCHING_BB_ID: &str = "39";
const PITCHING_H_ID: &str = "34";
const INNINGS_PITCHED_ID: &str = "50";
/// Confirmed `isScoring: false` in the feed itself — a display-only
/// combined stat (`H/AB`), not a scoring category. Never aggregated.
const NON_SCORING_DISPLAY_ONLY_ID: &str = "60";

const REDZONE_URL: &str = "https://pub-api.fantasysports.yahoo.com/fantasy/v3/redzone/mlb";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const BODY_LIMIT: usize = 8 * 1024 * 1024;
const HIDDEN_PLACEHOLDER: &str = "--hidden--";

/// One public-fetch failure with no credential material to leak.
#[derive(Debug)]
pub enum YahooPublicError {
    InvalidLeagueKey(String),
    Request(String),
    Blocked { status: u16 },
    Malformed(String),
    Incomplete(&'static str),
}

impl fmt::Display for YahooPublicError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLeagueKey(value) => write!(
                formatter,
                "resolve public league id: {value:?} is not a bare number or a {{game_key}}.l.{{league_id}} key; provide the numeric league id and retry"
            ),
            Self::Request(detail) => write!(
                formatter,
                "request Yahoo public feed: {detail}; verify connectivity and retry"
            ),
            Self::Blocked { status } => write!(
                formatter,
                "Yahoo public feed returned HTTP {status}; the feed may be temporarily unavailable or the league id may be wrong — retry later"
            ),
            Self::Malformed(detail) => write!(
                formatter,
                "parse Yahoo public feed: {detail}; the feed shape may have changed"
            ),
            Self::Incomplete(detail) => write!(
                formatter,
                "Yahoo public feed response is incomplete: {detail}; prior local data was retained"
            ),
        }
    }
}

impl std::error::Error for YahooPublicError {}

/// Extract the numeric Yahoo `league_id` from a bare number or a full
/// `{game_key}.l.{league_id}` key (the shape b9 config stores for `sync`).
pub fn league_id_from_key(value: &str) -> Result<String, YahooPublicError> {
    let trimmed = value.trim();
    let is_digits = |value: &str| !value.is_empty() && value.chars().all(|c| c.is_ascii_digit());
    if is_digits(trimmed) {
        return Ok(trimmed.to_owned());
    }
    if let Some((game_key, league_id)) = trimmed.split_once(".l.")
        && is_digits(game_key)
        && is_digits(league_id)
    {
        return Ok(league_id.to_owned());
    }
    Err(YahooPublicError::InvalidLeagueKey(value.to_owned()))
}

/// One complete public league snapshot ready for `FantasySnapshotWrite`.
#[derive(Clone, Debug, PartialEq)]
pub struct RedzoneFeed {
    pub league: League,
    pub teams: Vec<FantasyTeam>,
    pub players: Vec<FantasyPlayer>,
    pub slots: Vec<FantasyRosterSlot>,
    /// The current week's matchup pairings with aggregated category totals.
    /// Empty when the league has an odd team out or no matchups this week.
    pub matchups: Vec<Matchup>,
    /// Each team's full per-player weekly boxscore, keyed by `team_key`.
    pub roster_week_stats: HashMap<String, RosterWeekStats>,
    /// The current live scoring week, straight from `weekInfo.week` — the
    /// same value every observed `matchups` entry carries, exposed at the
    /// feed level so callers with no matchup this week (odd team out) still
    /// know what week they fetched.
    pub week: i32,
    /// Roster-slot counts, derived by tallying `league.positions`'
    /// real shape (each slot repeated once per label, e.g. `["OF","OF","OF"]`
    /// for 3 outfield slots) rather than pre-counted the way OAuth's own API
    /// already hands back — confirmed against a real captured response.
    pub roster_positions: Vec<RosterPosition>,
}

/// Unauthenticated client for the public redzone feed.
pub struct YahooPublicClient {
    http: HttpClient,
}

impl YahooPublicClient {
    /// Construct a client around an injected HTTP transport.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Construct a client using the production HTTPS transport.
    pub fn production() -> Result<Self, YahooPublicError> {
        HttpClient::production()
            .map(Self::new)
            .map_err(|error| YahooPublicError::Request(error.to_string()))
    }

    /// Fetch and normalize one league's public redzone data.
    ///
    /// Sends no cookies and no auth header — the request carries only a
    /// normal `Accept` header, matching what a plain browser request sends,
    /// never a spoofed identity.
    ///
    /// `league_id` selects the league on the wire; `league_key` is the b9
    /// storage key the resulting snapshot is written under — the caller
    /// decides that (reusing a real OAuth-derived key when one already
    /// resolves to the same league, synthesizing one otherwise), since only
    /// the caller knows what `sync`/`st`/etc. currently look up.
    pub fn fetch_redzone(
        &self,
        league_id: &str,
        league_key: &str,
    ) -> Result<RedzoneFeed, YahooPublicError> {
        let response = self
            .http
            .execute(HttpRequest {
                method: HttpMethod::Get,
                url: format!("{REDZONE_URL}?league_id={league_id}&format=json"),
                headers: vec![HttpHeader {
                    name: "Accept".into(),
                    value: "application/json".into(),
                }],
                body: Vec::new(),
                timeout: REQUEST_TIMEOUT,
                body_limit: BODY_LIMIT,
            })
            .map_err(|error| YahooPublicError::Request(error.to_string()))?;
        if response.status != 200 {
            return Err(YahooPublicError::Blocked {
                status: response.status,
            });
        }
        let raw: RawRoot = serde_json::from_slice(&response.body).map_err(|_| {
            YahooPublicError::Malformed("response is not the expected JSON shape".into())
        })?;
        raw.into_feed(league_id, league_key)
    }
}

#[derive(Deserialize)]
struct RawRoot {
    service: RawService,
}

#[derive(Deserialize)]
struct RawService {
    leagues: BTreeMap<String, RawLeague>,
    players: BTreeMap<String, RawPlayerLookup>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLeague {
    name: String,
    scoring_type: String,
    teams: BTreeMap<String, RawTeam>,
    week_info: RawWeekInfo,
    #[serde(default)]
    stats: Vec<RawStatMeta>,
    #[serde(default)]
    matchup_groups: Vec<RawMatchupGroup>,
    #[serde(default)]
    positions: Vec<String>,
}

#[derive(Deserialize)]
struct RawWeekInfo {
    week: i32,
    start: String,
    end: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStatMeta {
    id: String,
    is_scoring: bool,
    is_negative: bool,
}

#[derive(Deserialize)]
struct RawMatchupGroup {
    matchups: Vec<Vec<String>>,
}

#[derive(Deserialize)]
struct RawTeam {
    id: String,
    name: String,
    rank: String,
    wins: i64,
    losses: i64,
    ties: i64,
    managers: BTreeMap<String, RawManager>,
    players: Vec<RawRosterPlayer>,
}

#[derive(Deserialize)]
struct RawManager {
    #[serde(rename = "nickName")]
    nick_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRosterPlayer {
    // An empty roster slot comes back as a placeholder with `id: null`,
    // `positionType: false` (a bool, not a string), and `invalid: true`
    // rather than being omitted — never assume every slot holds a real
    // player.
    id: Option<String>,
    position: String,
    #[serde(default)]
    eligible_position_slots: Vec<String>,
    position_type: serde_json::Value,
    #[serde(default)]
    status: String,
    #[serde(default)]
    invalid: bool,
    // An invalid/empty slot's `stats` is `[]` (an array), not an object,
    // unlike every real player's `stats` map — never assume the shape is
    // consistent across placeholder vs. real rows.
    #[serde(default, deserialize_with = "deserialize_stats_map")]
    stats: BTreeMap<String, serde_json::Value>,
}

fn deserialize_stats_map<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Object(map) => map.into_iter().collect(),
        _ => BTreeMap::new(),
    })
}

#[derive(Deserialize)]
struct RawPlayerLookup {
    name: String,
    team: String,
}

fn stat_value(stats: &BTreeMap<String, serde_json::Value>, id: &str) -> f64 {
    stats
        .get(id)
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0)
}

fn stat_string(stats: &BTreeMap<String, serde_json::Value>, id: &str) -> String {
    let value = stat_value(stats, id);
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

/// Convert a whole out count into Yahoo's `.1`/`.2` fractional-inning
/// notation (thirds, never true decimal) — confirmed against two real
/// pitchers by reversing this exact conversion.
fn format_innings_pitched(outs: f64) -> String {
    let whole_outs = outs.round() as i64;
    format!("{}.{}", whole_outs / 3, whole_outs % 3)
}

/// Sum one team's active (non-bench, non-injured-list) roster stats.
/// Confirmed formulas: AVG = `ΣH ÷ ΣAB`; ERA = `9 × ΣER ÷ (ΣOUT ÷ 3)`;
/// WHIP = `(ΣBB + ΣH) ÷ (ΣOUT ÷ 3)` — verified against Yahoo's own reported
/// ERA/WHIP for two independent real pitchers to two decimal places.
fn aggregate_active_roster(team: &RawTeam) -> HashMap<&'static str, f64> {
    let mut sums: HashMap<&'static str, f64> = HashMap::new();
    for player in &team.players {
        if player.invalid || matches!(player.position.as_str(), "BN" | "IL") {
            continue;
        }
        for id in COUNTING_STAT_IDS {
            *sums.entry(id).or_default() += stat_value(&player.stats, id);
        }
    }
    sums
}

fn team_stats_display(sums: &HashMap<&'static str, f64>) -> HashMap<String, String> {
    let ab = sums.get(AB_ID).copied().unwrap_or(0.0);
    let batting_h = sums.get(BATTING_H_ID).copied().unwrap_or(0.0);
    let outs = sums.get(OUT_ID).copied().unwrap_or(0.0);
    let true_innings = outs / 3.0;
    let earned_runs = sums.get(EARNED_RUNS_ID).copied().unwrap_or(0.0);
    let pitching_bb = sums.get(PITCHING_BB_ID).copied().unwrap_or(0.0);
    let pitching_h = sums.get(PITCHING_H_ID).copied().unwrap_or(0.0);

    let mut display = HashMap::new();
    for (id, value) in sums {
        if *id == AB_ID || *id == BATTING_H_ID {
            // Building blocks for AVG only; not their own scoring category.
            continue;
        }
        let formatted = if value.fract() == 0.0 {
            format!("{value:.0}")
        } else {
            format!("{value}")
        };
        display.insert((*id).to_owned(), formatted);
    }
    display.insert(
        AVG_ID.to_owned(),
        if ab > 0.0 {
            format!("{:.3}", batting_h / ab)
        } else {
            "0.000".to_owned()
        },
    );
    display.insert(
        ERA_ID.to_owned(),
        if true_innings > 0.0 {
            format!("{:.2}", 9.0 * earned_runs / true_innings)
        } else {
            "0.00".to_owned()
        },
    );
    display.insert(
        WHIP_ID.to_owned(),
        if true_innings > 0.0 {
            format!("{:.2}", (pitching_bb + pitching_h) / true_innings)
        } else {
            "0.00".to_owned()
        },
    );
    display.insert(INNINGS_PITCHED_ID.to_owned(), format_innings_pitched(outs));
    display.remove(NON_SCORING_DISPLAY_ONLY_ID);
    display
}

/// Compare two teams' scoring categories and return (wins, losses, ties)
/// for `mine`, using each category's confirmed `isNegative` direction
/// (lower-is-better for ERA/WHIP, higher-is-better otherwise).
fn compare_categories(
    mine: &HashMap<String, String>,
    opponent: &HashMap<String, String>,
    scoring: &[RawStatMeta],
) -> (i32, i32, i32) {
    let (mut wins, mut losses, mut ties) = (0, 0, 0);
    for meta in scoring.iter().filter(|meta| meta.is_scoring) {
        let (Some(mine_value), Some(opponent_value)) = (mine.get(&meta.id), opponent.get(&meta.id))
        else {
            continue;
        };
        let (Ok(mine_value), Ok(opponent_value)) =
            (mine_value.parse::<f64>(), opponent_value.parse::<f64>())
        else {
            continue;
        };
        let mine_wins = if meta.is_negative {
            mine_value < opponent_value
        } else {
            mine_value > opponent_value
        };
        let opponent_wins = if meta.is_negative {
            opponent_value < mine_value
        } else {
            opponent_value > mine_value
        };
        if mine_wins {
            wins += 1;
        } else if opponent_wins {
            losses += 1;
        } else {
            ties += 1;
        }
    }
    (wins, losses, ties)
}

fn player_week_stats(
    raw_player: &RawRosterPlayer,
    lookup: Option<&RawPlayerLookup>,
) -> Option<PlayerWeekStats> {
    if raw_player.invalid {
        return None;
    }
    let yahoo_player_id = raw_player.id.as_ref()?.parse::<i64>().ok()?;
    let position_type = raw_player
        .position_type
        .as_str()
        .map_or_else(String::new, ToOwned::to_owned);
    let eligible_positions = raw_player
        .eligible_position_slots
        .iter()
        .map(|value| Position::from(value.as_str()))
        .collect();
    let outs = stat_value(&raw_player.stats, OUT_ID);
    let true_innings = outs / 3.0;
    let earned_runs = stat_value(&raw_player.stats, EARNED_RUNS_ID);
    let pitching_bb = stat_value(&raw_player.stats, PITCHING_BB_ID);
    let pitching_h = stat_value(&raw_player.stats, PITCHING_H_ID);
    let ab = stat_value(&raw_player.stats, AB_ID);
    let batting_h = stat_value(&raw_player.stats, BATTING_H_ID);
    Some(PlayerWeekStats {
        yahoo_player_id,
        name: lookup.map_or_else(String::new, |entry| entry.name.clone()),
        team: lookup.map_or_else(String::new, |entry| entry.team.clone()),
        position_type,
        slot_position: Position::from(raw_player.position.as_str()),
        eligible_positions,
        injury_status: raw_player.status.clone(),
        hab: format!(
            "{}-{}",
            stat_string(&raw_player.stats, BATTING_H_ID),
            stat_string(&raw_player.stats, AB_ID)
        ),
        runs: stat_value(&raw_player.stats, "7") as i32,
        home_runs: stat_value(&raw_player.stats, "12") as i32,
        runs_batted_in: stat_value(&raw_player.stats, "13") as i32,
        stolen_bases: stat_value(&raw_player.stats, "16") as i32,
        batting_average: if ab > 0.0 {
            format!("{:.3}", batting_h / ab)
        } else {
            "0.000".to_owned()
        },
        innings_pitched: format_innings_pitched(outs),
        wins: stat_value(&raw_player.stats, "28") as i32,
        saves: stat_value(&raw_player.stats, "32") as i32,
        strikeouts: stat_value(&raw_player.stats, "42") as i32,
        earned_run_average: if true_innings > 0.0 {
            format!("{:.2}", 9.0 * earned_runs / true_innings)
        } else {
            "0.00".to_owned()
        },
        whip: if true_innings > 0.0 {
            format!("{:.2}", (pitching_bb + pitching_h) / true_innings)
        } else {
            "0.00".to_owned()
        },
    })
}

impl RawRoot {
    fn into_feed(
        self,
        requested_league_id: &str,
        league_key: &str,
    ) -> Result<RedzoneFeed, YahooPublicError> {
        let raw_league =
            self.service
                .leagues
                .get(requested_league_id)
                .ok_or(YahooPublicError::Incomplete(
                    "response has no entry for the requested league id",
                ))?;
        if raw_league.teams.is_empty() {
            return Err(YahooPublicError::Incomplete("league has no teams"));
        }
        let season = raw_league
            .week_info
            .start
            .get(0..4)
            .and_then(|year| year.parse::<i32>().ok())
            .ok_or(YahooPublicError::Incomplete(
                "week start date is missing or malformed",
            ))?;
        let league_key = league_key.to_owned();
        let scoring_type = match raw_league.scoring_type.as_str() {
            "head" => ScoringType::HeadToHead,
            "point" | "points" => ScoringType::Points,
            "roto" | "rotisserie" => ScoringType::Rotisserie,
            other => ScoringType::Other(other.to_owned()),
        };
        let mut roster_positions: Vec<RosterPosition> = Vec::new();
        for label in &raw_league.positions {
            let position = Position::from(label.as_str());
            match roster_positions
                .iter_mut()
                .find(|entry| entry.position == position)
            {
                Some(entry) => entry.count += 1,
                None => roster_positions.push(RosterPosition { position, count: 1 }),
            }
        }
        let league = League {
            league_key: league_key.clone(),
            name: raw_league.name.clone(),
            season,
            num_teams: i32::try_from(raw_league.teams.len()).unwrap_or(0),
            scoring_type,
            roster_positions: Vec::new(),
            batting_categories: Vec::new(),
            pitching_categories: Vec::new(),
        };
        let mut teams = Vec::with_capacity(raw_league.teams.len());
        let mut players: BTreeMap<i64, FantasyPlayer> = BTreeMap::new();
        let mut slots = Vec::new();
        let mut team_stats_by_id: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut roster_week_stats: HashMap<String, RosterWeekStats> = HashMap::new();
        for raw_team in raw_league.teams.values() {
            let team_key = format!("{league_key}.t.{}", raw_team.id);
            let team_id = raw_team
                .id
                .parse::<i64>()
                .map_err(|_| YahooPublicError::Incomplete("team id is not numeric"))?;
            let rank = raw_team.rank.parse::<i64>().unwrap_or(0);
            let manager_name = raw_team.managers.values().next().map_or_else(
                || HIDDEN_PLACEHOLDER.to_owned(),
                |manager| manager.nick_name.clone(),
            );
            teams.push(FantasyTeam {
                team_key: team_key.clone(),
                league_key: league_key.clone(),
                team_id,
                name: raw_team.name.clone(),
                manager_name,
                is_owned_by_current_login: false,
                waiver_priority: 0,
                faab_balance: 0,
                wins: raw_team.wins,
                losses: raw_team.losses,
                ties: raw_team.ties,
                moves: 0,
                rank,
            });
            for raw_player in &raw_team.players {
                // Empty roster slots are placeholders (`invalid: true`, `id: null`),
                // not real players — skip them rather than fabricate an entry.
                if raw_player.invalid {
                    continue;
                }
                let Some(raw_id) = &raw_player.id else {
                    continue;
                };
                let yahoo_player_id = raw_id
                    .parse::<i64>()
                    .map_err(|_| YahooPublicError::Incomplete("player id is not numeric"))?;
                let lookup = self.service.players.get(raw_id);
                let eligible_positions = raw_player
                    .eligible_position_slots
                    .iter()
                    .map(|value| Position::from(value.as_str()))
                    .collect();
                let position_type = raw_player
                    .position_type
                    .as_str()
                    .map_or_else(String::new, ToOwned::to_owned);
                players
                    .entry(yahoo_player_id)
                    .or_insert_with(|| FantasyPlayer {
                        yahoo_player_id,
                        name: lookup.map_or_else(String::new, |entry| entry.name.clone()),
                        mlb_team: lookup.map_or_else(String::new, |entry| entry.team.clone()),
                        display_position: raw_player.position.clone(),
                        position_type,
                        eligible_positions,
                        injury_status: raw_player.status.clone(),
                        percent_owned: None,
                        yahoo_rank: None,
                    });
                slots.push(FantasyRosterSlot {
                    team_key: team_key.clone(),
                    yahoo_player_id,
                    slot_position: Position::from(raw_player.position.as_str()),
                });
            }
            team_stats_by_id.insert(
                raw_team.id.clone(),
                team_stats_display(&aggregate_active_roster(raw_team)),
            );
            roster_week_stats.insert(
                team_key.clone(),
                RosterWeekStats {
                    team_key: team_key.clone(),
                    team_name: raw_team.name.clone(),
                    week: raw_league.week_info.week,
                    players: raw_team
                        .players
                        .iter()
                        .filter_map(|raw_player| {
                            player_week_stats(
                                raw_player,
                                raw_player
                                    .id
                                    .as_deref()
                                    .and_then(|id| self.service.players.get(id)),
                            )
                        })
                        .collect(),
                },
            );
        }
        if players.is_empty() || slots.is_empty() {
            return Err(YahooPublicError::Incomplete(
                "league has no rostered players",
            ));
        }
        let matchups = raw_league
            .matchup_groups
            .iter()
            .flat_map(|group| group.matchups.iter())
            .filter_map(|pair| {
                let [team_a, team_b] = <[String; 2]>::try_from(pair.clone()).ok()?;
                let raw_a = raw_league.teams.get(&team_a)?;
                let raw_b = raw_league.teams.get(&team_b)?;
                let stats_a = team_stats_by_id.get(&team_a)?.clone();
                let stats_b = team_stats_by_id.get(&team_b)?.clone();
                let (wins_a, losses_a, ties_a) =
                    compare_categories(&stats_a, &stats_b, &raw_league.stats);
                let team_key_a = format!("{league_key}.t.{team_a}");
                let team_key_b = format!("{league_key}.t.{team_b}");
                Some(Matchup {
                    week: raw_league.week_info.week,
                    week_start: raw_league.week_info.start.clone(),
                    week_end: raw_league.week_info.end.clone(),
                    status: String::new(),
                    teams: [
                        MatchupTeam {
                            team_key: team_key_a,
                            team_id: team_a.parse().unwrap_or(0),
                            name: raw_a.name.clone(),
                            is_current_login: false,
                            stats: stats_a,
                            wins: wins_a,
                            losses: losses_a,
                            ties: ties_a,
                            completed_games: 0,
                            live_games: 0,
                            remaining_games: 0,
                        },
                        MatchupTeam {
                            team_key: team_key_b,
                            team_id: team_b.parse().unwrap_or(0),
                            name: raw_b.name.clone(),
                            is_current_login: false,
                            stats: stats_b,
                            wins: losses_a,
                            losses: wins_a,
                            ties: ties_a,
                            completed_games: 0,
                            live_games: 0,
                            remaining_games: 0,
                        },
                    ],
                })
            })
            .collect();
        Ok(RedzoneFeed {
            league,
            teams,
            players: players.into_values().collect(),
            slots,
            matchups,
            roster_week_stats,
            week: raw_league.week_info.week,
            roster_positions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn league_id_from_key_accepts_bare_numbers_and_full_keys() {
        assert_eq!(league_id_from_key("170874").unwrap(), "170874");
        assert_eq!(league_id_from_key("469.l.170874").unwrap(), "170874");
        assert_eq!(league_id_from_key("  170874  ").unwrap(), "170874");
    }

    #[test]
    fn league_id_from_key_rejects_malformed_input() {
        assert!(league_id_from_key("").is_err());
        assert!(league_id_from_key("mlb.l.170874x").is_err());
        assert!(league_id_from_key("469.l.").is_err());
        assert!(league_id_from_key(".l.170874").is_err());
        assert!(league_id_from_key("not-a-key").is_err());
    }
}
