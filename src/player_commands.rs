//! Foreground roster and player-pool command orchestration.

use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use crate::config;
use crate::domain::{Matchup, PlayerGameLog, StoredFantasyPlayer};
use crate::evaluation::sort_by_evaluation;
use crate::player_display::{
    render_detail, render_league_totals, render_players, render_weekly_totals,
};
use crate::providers::mlb::MlbClient;
use crate::providers::yahoo::YahooClient;
use crate::providers::yahoo_fantasy::{YahooFantasyClient, YahooFantasySource};
use crate::store::{Store, StoredFantasyTeam, WaiverCandidate};
use crate::store::{SyncMode, SyncRunStatus};
use crate::terminal::detected_help_color_mode;
use crate::transport::HttpClient;

/// One roster and player-pool command failure.
#[derive(Debug)]
pub struct PlayerCommandError(String);
impl fmt::Display for PlayerCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for PlayerCommandError {}
fn error(command: &str, detail: impl fmt::Display) -> PlayerCommandError {
    PlayerCommandError(format!("{command}: {detail}; run b9 sync and retry"))
}

fn context() -> Result<(Store, String, String), PlayerCommandError> {
    let config = config::read().map_err(|failure| error("player", failure))?;
    if config.current_league.is_empty() {
        return Err(error("player", "no league selected; run b9 st -l <key>"));
    }
    let mut store = Store::open().map_err(|failure| error("player", failure))?;
    let _ = store
        .bootstrap_legacy()
        .map_err(|failure| error("player", failure))?;
    Ok((store, config.current_league, config.current_team_key))
}

/// Render a configured or queried fantasy roster.
pub fn show_roster(query: Option<&str>) -> Result<String, PlayerCommandError> {
    let (store, league, selected) = context()?;
    let teams = store
        .fantasy_teams(&league)
        .map_err(|failure| error("r", failure))?;
    let team = select_roster_team(&teams, &selected, query)?;
    let players = store
        .fantasy_players(&league)
        .map_err(|failure| error("r", failure))?
        .into_iter()
        .filter(|player| player.owner.as_deref() == Some(team.name.as_str()))
        .collect::<Vec<_>>();
    if players.is_empty() {
        return Err(error(
            "r",
            "the selected team has no durable roster snapshot",
        ));
    }
    let mut players = players;
    sort_roster_players(&mut players);
    populate_game_statuses(&mut players);
    let output = render_players(&team.name, &players, detected_help_color_mode());
    yahoo_result_notice(&store, output)
}

fn populate_game_statuses(players: &mut [StoredFantasyPlayer]) {
    let Ok(http) = HttpClient::production() else {
        return;
    };
    let client = MlbClient::production(Arc::new(http));
    let date = utc_date(SystemTime::now());
    let Ok(games) = client.fetch_schedule(&date) else {
        return;
    };
    apply_game_statuses(players, &games);
}

fn apply_game_statuses(
    players: &mut [StoredFantasyPlayer],
    games: &[crate::providers::mlb::ScheduleGame],
) {
    for player in players.iter_mut().filter(|player| player.status.is_empty()) {
        let Some(game) = games.iter().find(|game| {
            let away = mlb_team_abbreviation(game.away_team_id);
            let home = mlb_team_abbreviation(game.home_team_id);
            player.team.eq_ignore_ascii_case(away) || player.team.eq_ignore_ascii_case(home)
        }) else {
            continue;
        };
        let away = player
            .team
            .eq_ignore_ascii_case(mlb_team_abbreviation(game.away_team_id));
        let opponent = if away {
            mlb_team_abbreviation(game.home_team_id)
        } else {
            mlb_team_abbreviation(game.away_team_id)
        };
        let location = if away {
            format!("@ {opponent}")
        } else {
            format!("vs {opponent}")
        };
        player.game_status = if game.detailed_state.eq_ignore_ascii_case("final") {
            game.linescore.as_ref().map_or_else(
                || format!("Final {location}"),
                |score| format!("Final {}-{} {location}", score.away_runs, score.home_runs),
            )
        } else if let Some(score) = &game.linescore {
            format!(
                "{}{} {}-{} {location}",
                score.inning_state.chars().next().unwrap_or('T'),
                score.inning_ordinal,
                score.away_runs,
                score.home_runs
            )
        } else {
            format!(
                "{} {location}",
                game.game_date.get(11..16).unwrap_or("Scheduled")
            )
        };
    }
}

fn utc_date(now: SystemTime) -> String {
    let days = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        / 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn mlb_team_abbreviation(team_id: i64) -> &'static str {
    match team_id {
        108 => "LAA",
        109 => "ARI",
        110 => "BAL",
        111 => "BOS",
        112 => "CHC",
        113 => "CIN",
        114 => "CLE",
        115 => "COL",
        116 => "DET",
        117 => "HOU",
        118 => "KC",
        119 => "LAD",
        120 => "WSH",
        121 => "NYM",
        133 => "OAK",
        134 => "PIT",
        135 => "SD",
        136 => "SEA",
        137 => "SF",
        138 => "STL",
        139 => "TB",
        140 => "TEX",
        141 => "TOR",
        142 => "MIN",
        143 => "PHI",
        144 => "ATL",
        145 => "CWS",
        146 => "MIA",
        147 => "NYY",
        158 => "MIL",
        _ => "",
    }
}

fn sort_roster_players(players: &mut [StoredFantasyPlayer]) {
    fn slot_order(slot: Option<&str>) -> usize {
        match slot.unwrap_or("").to_ascii_uppercase().as_str() {
            "C" => 0,
            "1B" => 1,
            "2B" => 2,
            "3B" => 3,
            "SS" => 4,
            "OF" | "LF" | "CF" | "RF" => 5,
            "UTIL" => 6,
            "SP" => 7,
            "RP" => 8,
            "P" => 9,
            "BN" => 10,
            "IL" | "IL10" | "IL15" | "IL60" => 11,
            _ => 12,
        }
    }

    players.sort_by(|left, right| {
        slot_order(left.slot.as_deref())
            .cmp(&slot_order(right.slot.as_deref()))
            .then_with(|| left.name.cmp(&right.name))
    });
}

fn select_roster_team<'a>(
    teams: &'a [StoredFantasyTeam],
    selected: &str,
    query: Option<&str>,
) -> Result<&'a StoredFantasyTeam, PlayerCommandError> {
    let needle = query.unwrap_or(selected).to_lowercase();
    let matches = teams
        .iter()
        .filter(|team| {
            (query.is_some()
                && (team.name.to_lowercase().contains(&needle)
                    || team.manager_name.to_lowercase().contains(&needle)))
                || (query.is_none() && team.team_key == selected)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [team] => Ok(*team),
        [] => Err(error("r", "no team matches the query")),
        _ => Err(error(
            "r",
            format!(
                "query is ambiguous; matches: {}",
                matches
                    .iter()
                    .map(|team| team.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

/// Render configured roster totals from durable state.
pub fn show_totals(weekly: Option<&str>) -> Result<String, PlayerCommandError> {
    let (mut store, league, selected) = context()?;
    if let Some(requested) = weekly {
        return show_weekly_totals(&mut store, &league, &selected, requested);
    }
    let teams = store
        .fantasy_teams(&league)
        .map_err(|failure| error("rt", failure))?;
    let players = store
        .fantasy_players(&league)
        .map_err(|failure| error("rt", failure))?;
    let output = render_league_totals(&teams, &players, detected_help_color_mode());
    yahoo_result_notice(&store, output)
}

fn show_weekly_totals(
    store: &mut Store,
    league: &str,
    team_key: &str,
    requested: &str,
) -> Result<String, PlayerCommandError> {
    let http = Arc::new(HttpClient::production().map_err(|failure| error("rt", failure))?);
    let yahoo = Arc::new(YahooClient::production(http).map_err(|failure| error("rt", failure))?);
    let source = YahooFantasyClient::new(yahoo);
    let current_week = store
        .fantasy_current_week(league)
        .map_err(|failure| error("rt", failure))?
        .ok_or_else(|| error("rt", "league current week is unavailable"))?;
    let (matchup, stale) = if requested == "true" {
        weekly_matchup(store, &source, league, current_week)?
    } else if let Ok(week) = requested.parse::<i32>() {
        if week <= 0 {
            return Err(error("rt", "week must be positive"));
        }
        weekly_matchup(store, &source, league, week)?
    } else {
        resolve_date_matchup(store, &source, league, current_week, requested)?
    };
    let team = matchup
        .teams
        .iter()
        .find(|team| team.team_key == team_key)
        .ok_or_else(|| error("rt", "selected team has no weekly matchup"))?;
    let categories = store
        .fantasy_categories(league)
        .map_err(|failure| error("rt", failure))?;
    Ok(render_weekly_totals(
        &team.name,
        &format!("WEEK {}", matchup.week),
        team,
        &categories,
        stale,
        detected_help_color_mode(),
    ))
}

fn weekly_matchup(
    store: &mut Store,
    source: &impl YahooFantasySource,
    league: &str,
    week: i32,
) -> Result<(Matchup, bool), PlayerCommandError> {
    let scope = format!("{league}:{week}");
    let (matchups, stale) = crate::matchup::cached_or_fetch_at(
        store,
        "match_scoreboard",
        &scope,
        SystemTime::now(),
        || source.scoreboard(league, Some(week)),
    )
    .map_err(|failure| error("rt", failure))?;
    let matchup = matchups
        .into_iter()
        .find(|matchup| matchup.week == week)
        .ok_or_else(|| error("rt", "requested week has no matchup"))?;
    Ok((matchup, stale))
}

fn resolve_date_matchup(
    store: &mut Store,
    source: &impl YahooFantasySource,
    league: &str,
    current_week: i32,
    date: &str,
) -> Result<(Matchup, bool), PlayerCommandError> {
    if !is_iso_date(date) {
        return Err(error("rt", "weekly date must use YYYY-MM-DD"));
    }
    for week in 1..=current_week {
        let (matchup, stale) = weekly_matchup(store, source, league, week)?;
        if matchup.week_start.as_str() <= date && date <= matchup.week_end.as_str() {
            return Ok((matchup, stale));
        }
    }
    Err(error(
        "rt",
        "date is outside the available Yahoo matchup weeks",
    ))
}

fn is_iso_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

/// Render a hitter or pitcher player pool, or a player detail card.
pub fn show_pool(
    role: &str,
    argument: Option<&str>,
    sort: Option<&str>,
    position: Option<&str>,
    waiver: bool,
) -> Result<String, PlayerCommandError> {
    let (mut store, league, _) = context()?;
    let mut players = store
        .fantasy_players(&league)
        .map_err(|failure| error(role, failure))?;
    if let Some(query) = argument.filter(|value| value.parse::<usize>().is_err()) {
        let matches = players
            .iter()
            .filter(|player| player.name.to_lowercase().contains(&query.to_lowercase()))
            .collect::<Vec<_>>();
        return match matches.as_slice() {
            [player] => {
                let season = store
                    .fantasy_season(&league)
                    .map_err(|failure| error(role, failure))?
                    .ok_or_else(|| error(role, "league season is unavailable"))?;
                let (logs, stale) = game_logs(&mut store, player, season)?;
                let output = render_detail(
                    player,
                    &logs,
                    stale,
                    &utc_date(SystemTime::now()),
                    detected_help_color_mode(),
                );
                yahoo_result_notice(&store, output)
            }
            [] => Err(error(role, "no player matches the query")),
            _ => Err(error(
                role,
                format!(
                    "player query is ambiguous; matches: {}",
                    matches
                        .iter()
                        .map(|player| player.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )),
        };
    }
    let waiver_candidates = if waiver {
        let candidates = store
            .waiver_candidates()
            .map_err(|failure| error(role, failure))?;
        if candidates.is_empty() {
            return Err(error(
                role,
                "active MLB roster data is unavailable; run b9 sync and retry",
            ));
        }
        Some(candidates)
    } else {
        None
    };
    players.retain(|player| {
        player.role == role
            && position.is_none_or(|value| {
                player
                    .positions
                    .split(',')
                    .any(|position| position.eq_ignore_ascii_case(value))
            })
            && waiver_candidates
                .as_ref()
                .is_none_or(|candidates| waiver_eligible(player, position, candidates))
    });
    if waiver && sort.is_none() {
        sort_by_evaluation(&mut players);
    } else {
        sort_pool_players(&mut players, sort.unwrap_or("rank"));
    }
    let limit = argument
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20);
    players.truncate(limit);
    let output = render_players(
        if role == "B" { "HITTERS" } else { "PITCHERS" },
        &players,
        detected_help_color_mode(),
    );
    yahoo_result_notice(&store, output)
}

fn sort_pool_players(players: &mut [StoredFantasyPlayer], field: &str) {
    match field.to_ascii_lowercase().as_str() {
        "name" | "player" => players.sort_by(|left, right| left.name.cmp(&right.name)),
        "owner" => players.sort_by(|left, right| left.owner.cmp(&right.owner)),
        "position" | "pos" => players.sort_by(|left, right| left.positions.cmp(&right.positions)),
        "team" => players.sort_by(|left, right| left.team.cmp(&right.team)),
        "yr" | "rank" => players.sort_by_key(|player| player.rank.unwrap_or(i64::MAX)),
        "pa" => players.sort_by(|left, right| right.batting[0].total_cmp(&left.batting[0])),
        "ip" => players.sort_by(|left, right| right.pitching[0].total_cmp(&left.pitching[0])),
        _ => players.sort_by_key(|player| player.rank.unwrap_or(i64::MAX)),
    }
}

fn yahoo_result_notice(store: &Store, output: String) -> Result<String, PlayerCommandError> {
    let status = store
        .latest_sync_run(SyncMode::Live)
        .map_err(|failure| error("player", failure))?
        .map(|run| run.status);
    Ok(match status {
        Some(SyncRunStatus::Complete) => with_yahoo_result_notice(false, output),
        Some(SyncRunStatus::Failed) => with_yahoo_result_notice(true, output),
        Some(SyncRunStatus::Running) | None => output,
    })
}

/// Label retained Yahoo output without repeating root-help attribution.
pub fn with_yahoo_result_notice(stale: bool, output: String) -> String {
    if stale {
        format!(
            "STALE — showing the last complete Yahoo roster and player-pool snapshot.\n{output}"
        )
    } else {
        output
    }
}

/// Return whether one player passes the active-roster waiver gate.
pub fn waiver_eligible(
    player: &StoredFantasyPlayer,
    requested_position: Option<&str>,
    candidates: &[WaiverCandidate],
) -> bool {
    if player.owner.is_some()
        || player.status.starts_with("IL")
        || player.status.eq_ignore_ascii_case("NA")
        || player.status.eq_ignore_ascii_case("SUSP")
    {
        return false;
    }
    let Some(id) = player.mlbam_id else {
        return false;
    };
    if player.role == "B" {
        let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.mlbam_id == id && candidate.role == "H")
        else {
            return false;
        };
        let mut matching_floors = hitter_positions(player)
            .into_iter()
            .filter_map(|position| {
                requested_position
                    .is_none_or(|requested| requested.eq_ignore_ascii_case(position))
                    .then(|| hitter_floor(candidates, position))
                    .flatten()
            })
            .collect::<Vec<_>>();
        if let Some(requested) = requested_position
            && matching_floors.is_empty()
        {
            matching_floors.push(hitter_floor(candidates, requested).unwrap_or_else(|| {
                percentile(
                    candidates
                        .iter()
                        .filter(|candidate| candidate.role == "H")
                        .map(|candidate| candidate.plate_appearances)
                        .filter(|value| *value > 0.0)
                        .collect(),
                )
                .unwrap_or(f64::INFINITY)
            }));
        }
        candidate.plate_appearances
            >= matching_floors
                .into_iter()
                .reduce(f64::min)
                .unwrap_or(f64::INFINITY)
    } else {
        let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.mlbam_id == id && candidate.role == "P")
        else {
            return false;
        };
        let starter = candidate.games_started.saturating_mul(2) >= candidate.games;
        let floor = percentile(
            candidates
                .iter()
                .filter(|candidate| {
                    candidate.role == "P"
                        && (candidate.games_started.saturating_mul(2) >= candidate.games) == starter
                })
                .map(|candidate| candidate.innings_pitched)
                .filter(|value| *value > 0.0)
                .collect(),
        )
        .unwrap_or(f64::INFINITY);
        candidate.innings_pitched >= floor
    }
}

fn hitter_positions(player: &StoredFantasyPlayer) -> Vec<&str> {
    player
        .positions
        .split(',')
        .filter(|position| matches!(*position, "C" | "1B" | "2B" | "3B" | "SS" | "OF"))
        .collect()
}

fn hitter_floor(candidates: &[WaiverCandidate], position: &str) -> Option<f64> {
    let values = candidates
        .iter()
        .filter(|candidate| {
            candidate.role == "H"
                && candidate
                    .positions
                    .split(',')
                    .any(|eligible| eligible.eq_ignore_ascii_case(position))
        })
        .map(|candidate| candidate.plate_appearances)
        .filter(|value| *value > 0.0)
        .collect();
    percentile(values)
}

fn percentile(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let rank = 0.60 * (values.len() - 1) as f64;
    let low = rank.floor() as usize;
    let high = rank.ceil() as usize;
    Some(values[low] + (rank - low as f64) * (values[high] - values[low]))
}

fn game_logs(
    store: &mut Store,
    player: &StoredFantasyPlayer,
    season: i64,
) -> Result<(Vec<PlayerGameLog>, bool), PlayerCommandError> {
    const DATASET: &str = "player-game-log";
    let person_id = player
        .mlbam_id
        .ok_or_else(|| error("player detail", "MLB identity is unavailable"))?;
    let scope = person_id.to_string();
    let refreshed = HttpClient::production()
        .map_err(|failure| failure.to_string())
        .and_then(|client| {
            let client = MlbClient::production(std::sync::Arc::new(client));
            if player.role == "P" {
                client
                    .fetch_pitcher_game_log(person_id, season)
                    .map(|entries| {
                        entries
                            .into_iter()
                            .map(|entry| PlayerGameLog {
                                date: entry.date,
                                opponent: format!(
                                    "{} {}",
                                    if entry.is_home { "vs" } else { "@" },
                                    entry.opponent_abbreviation
                                ),
                                line: format!(
                                    "IP {}  W {}  SV {}  K {}  ERA {}  WHIP {}",
                                    entry.stat.innings_pitched,
                                    entry.stat.wins,
                                    entry.stat.saves,
                                    entry.stat.strikeouts,
                                    entry.stat.era,
                                    entry.stat.whip
                                ),
                            })
                            .collect()
                    })
            } else {
                client
                    .fetch_hitter_game_log(person_id, season)
                    .map(|entries| {
                        entries
                            .into_iter()
                            .map(|entry| PlayerGameLog {
                                date: entry.date,
                                opponent: format!(
                                    "{} {}",
                                    if entry.is_home { "vs" } else { "@" },
                                    entry.opponent_abbreviation
                                ),
                                line: format!(
                                    "AB {}  H {}  R {}  HR {}  RBI {}  SB {}  AVG {}",
                                    entry.stat.at_bats,
                                    entry.stat.hits,
                                    entry.stat.runs,
                                    entry.stat.home_runs,
                                    entry.stat.rbi,
                                    entry.stat.stolen_bases,
                                    entry.stat.average,
                                ),
                            })
                            .collect()
                    })
            }
            .map_err(|failure| failure.to_string())
        });
    match refreshed {
        Ok(logs) => {
            let payload =
                serde_json::to_string(&logs).map_err(|failure| error("player detail", failure))?;
            store
                .save_command_snapshot(DATASET, "mlb", &scope, "v1", &payload)
                .map_err(|failure| error("player detail", failure))?;
            Ok((logs, false))
        }
        Err(failure) => {
            store
                .mark_command_snapshot_stale(DATASET, "mlb", &scope, &failure.to_string())
                .map_err(|mark| error("player detail", mark))?;
            let snapshot = store
                .command_snapshot(DATASET, "mlb", &scope)
                .map_err(|read| error("player detail", read))?
                .ok_or_else(|| {
                    error("player detail", format!("game log unavailable ({failure})"))
                })?;
            let logs = serde_json::from_str(&snapshot.payload)
                .map_err(|decode| error("player detail", decode))?;
            Ok((logs, true))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use tempfile::tempdir;

    use super::{
        apply_game_statuses, resolve_date_matchup, select_roster_team, sort_pool_players,
        weekly_matchup, yahoo_result_notice,
    };
    use crate::domain::{
        FantasyPlayer, FantasyTeam, Matchup, MatchupTeam, RosterWeekStats, StoredFantasyPlayer,
    };
    use crate::providers::mlb::{Linescore, ScheduleGame};
    use crate::providers::yahoo_fantasy::{
        LeagueRosters, LeagueSettings, UserLeague, YahooFantasyError, YahooFantasySource,
    };
    use crate::store::{Store, StoredFantasyTeam};

    struct WeeklySource {
        requested_weeks: RefCell<Vec<i32>>,
    }

    impl WeeklySource {
        fn matchup(week: i32, start: &str, end: &str) -> Matchup {
            let team = |key: &str, name: &str| MatchupTeam {
                team_key: key.into(),
                team_id: 1,
                name: name.into(),
                is_current_login: key == "mlb.l.1.t.1",
                stats: HashMap::new(),
                wins: 0,
                losses: 0,
                ties: 0,
                completed_games: 0,
                live_games: 0,
                remaining_games: 0,
            };
            Matchup {
                week,
                week_start: start.into(),
                week_end: end.into(),
                status: "postevent".into(),
                teams: [team("mlb.l.1.t.1", "One"), team("mlb.l.1.t.2", "Two")],
            }
        }
    }

    impl YahooFantasySource for WeeklySource {
        fn user_leagues(&self) -> Result<Vec<UserLeague>, YahooFantasyError> {
            Err(YahooFantasyError::Incomplete("not used"))
        }
        fn team_key(&self, _: &str) -> Result<String, YahooFantasyError> {
            Err(YahooFantasyError::Incomplete("not used"))
        }
        fn league_settings(&self, _: &str) -> Result<LeagueSettings, YahooFantasyError> {
            Err(YahooFantasyError::Incomplete("not used"))
        }
        fn standings(&self, _: &str) -> Result<Vec<FantasyTeam>, YahooFantasyError> {
            Err(YahooFantasyError::Incomplete("not used"))
        }
        fn league_rosters(&self, _: &str) -> Result<LeagueRosters, YahooFantasyError> {
            Err(YahooFantasyError::Incomplete("not used"))
        }
        fn free_agents(&self, _: &str) -> Result<Vec<FantasyPlayer>, YahooFantasyError> {
            Err(YahooFantasyError::Incomplete("not used"))
        }
        fn scoreboard(
            &self,
            _: &str,
            week: Option<i32>,
        ) -> Result<Vec<Matchup>, YahooFantasyError> {
            let week = week.expect("weekly request supplies a week");
            self.requested_weeks.borrow_mut().push(week);
            let matchup = match week {
                1 => Self::matchup(1, "2026-03-26", "2026-04-05"),
                2 => Self::matchup(2, "2026-04-06", "2026-04-12"),
                _ => return Err(YahooFantasyError::Incomplete("unknown week")),
            };
            Ok(vec![matchup])
        }
        fn roster_week_stats(&self, _: &str, _: i32) -> Result<RosterWeekStats, YahooFantasyError> {
            Err(YahooFantasyError::Incomplete("not used"))
        }
    }

    #[test]
    fn weekly_resolution_fetches_numbered_and_iso_date_periods() {
        let directory = tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let source = WeeklySource {
            requested_weeks: RefCell::new(Vec::new()),
        };

        let (numbered, stale) = weekly_matchup(&mut store, &source, "mlb.l.1", 2).unwrap();
        assert_eq!(numbered.week, 2);
        assert!(!stale);

        let (dated, stale) =
            resolve_date_matchup(&mut store, &source, "mlb.l.1", 2, "2026-04-09").unwrap();
        assert_eq!(dated.week, 2);
        assert!(!stale);
        assert_eq!(*source.requested_weeks.borrow(), vec![2, 1]);
    }

    #[test]
    fn roster_selection_uses_default_and_reports_ambiguous_matches() {
        let teams = vec![
            StoredFantasyTeam {
                team_key: "mlb.l.1.t.1".into(),
                name: "North Stars".into(),
                manager_name: "Ada".into(),
                team_id: 1,
                waiver_priority: 1,
                faab_balance: 50,
                wins: 10,
                losses: 5,
                ties: 1,
                moves: 12,
                rank: 1,
            },
            StoredFantasyTeam {
                team_key: "mlb.l.1.t.2".into(),
                name: "South Stars".into(),
                manager_name: "Grace".into(),
                team_id: 2,
                waiver_priority: 2,
                faab_balance: 40,
                wins: 8,
                losses: 7,
                ties: 1,
                moves: 10,
                rank: 2,
            },
        ];
        assert_eq!(
            select_roster_team(&teams, "mlb.l.1.t.2", None)
                .unwrap()
                .name,
            "South Stars"
        );
        assert_eq!(
            select_roster_team(&teams, "", Some("ada")).unwrap().name,
            "North Stars"
        );
        assert!(
            select_roster_team(&teams, "", Some("stars"))
                .unwrap_err()
                .to_string()
                .contains("North Stars, South Stars")
        );
    }

    #[test]
    fn pool_sorting_covers_every_displayed_column() {
        let mut players = vec![
            crate::domain::StoredFantasyPlayer {
                yahoo_player_id: Some(1),
                mlbam_id: Some(1),
                name: "Zulu".into(),
                team: "NYY".into(),
                role: "B".into(),
                positions: "OF".into(),
                status: String::new(),
                injury_note: String::new(),
                birth_date: String::new(),
                game_status: String::new(),
                hand: String::new(),
                rank: Some(9),
                percent_owned: None,
                owner: Some("Zulu Owner".into()),
                slot: None,
                batting: [1.0; 7],
                pitching: [0.0; 7],
                hitting_advanced: [None; 8],
                pitching_advanced: [None; 6],
            },
            crate::domain::StoredFantasyPlayer {
                yahoo_player_id: Some(2),
                mlbam_id: Some(2),
                name: "Alpha".into(),
                team: "BOS".into(),
                role: "B".into(),
                positions: "C".into(),
                status: String::new(),
                injury_note: String::new(),
                birth_date: String::new(),
                game_status: String::new(),
                hand: String::new(),
                rank: Some(1),
                percent_owned: None,
                owner: None,
                slot: None,
                batting: [2.0; 7],
                pitching: [0.0; 7],
                hitting_advanced: [None; 8],
                pitching_advanced: [None; 6],
            },
        ];
        for (field, expected) in [
            ("name", "Alpha"),
            ("pos", "Alpha"),
            ("team", "Alpha"),
            ("yr", "Alpha"),
            ("owner", "Alpha"),
            ("pa", "Alpha"),
        ] {
            sort_pool_players(&mut players, field);
            assert_eq!(players[0].name, expected, "{field}");
        }
    }

    #[test]
    fn legacy_only_output_is_not_attributed_as_live_yahoo_data() {
        let directory = tempdir().unwrap();
        let store = Store::open_at(directory.path().join("b9.db")).unwrap();
        assert_eq!(
            yahoo_result_notice(&store, "POOL\n".into()).unwrap(),
            "POOL\n"
        );
    }

    #[test]
    fn game_state_fills_only_players_without_injury_status() {
        let player = |status: &str| StoredFantasyPlayer {
            yahoo_player_id: Some(1),
            mlbam_id: Some(1),
            name: "Ada".into(),
            team: "NYY".into(),
            role: "B".into(),
            positions: "OF".into(),
            status: status.into(),
            injury_note: String::new(),
            birth_date: String::new(),
            game_status: String::new(),
            hand: "R".into(),
            rank: None,
            percent_owned: None,
            owner: None,
            slot: None,
            batting: [0.0; 7],
            pitching: [0.0; 7],
            hitting_advanced: [None; 8],
            pitching_advanced: [None; 6],
        };
        let game = ScheduleGame {
            game_id: 1,
            game_date: "2026-08-17T19:05:00Z".into(),
            detailed_state: "Final".into(),
            away_team_id: 147,
            away_team_name: "Yankees".into(),
            home_team_id: 111,
            home_team_name: "Red Sox".into(),
            away_probable_pitcher_id: None,
            away_probable_pitcher_name: String::new(),
            home_probable_pitcher_id: None,
            home_probable_pitcher_name: String::new(),
            linescore: Some(Linescore {
                inning: Some(9),
                inning_ordinal: "9th".into(),
                inning_state: "End".into(),
                away_runs: 4,
                home_runs: 2,
            }),
            away_lineup: None,
            home_lineup: None,
        };
        let mut players = [player(""), player("DTD")];
        apply_game_statuses(&mut players, &[game]);
        assert_eq!(players[0].game_status, "Final 4-2 @ BOS");
        assert!(players[1].game_status.is_empty());
    }
}
