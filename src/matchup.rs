//! Lazy Yahoo matchup acquisition, durable fallback, view assembly, and rendering.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde::{Serialize, de::DeserializeOwned};

use crate::config;
use crate::domain::{Matchup, MatchupTeam, PlayerWeekStats, RosterWeekStats};
use crate::providers::yahoo::YahooClient;
use crate::providers::yahoo_fantasy::{YahooFantasyClient, YahooFantasySource};
use crate::store::Store;
use crate::terminal::{HelpColorMode, detected_help_color_mode, section, title};
use crate::transport::HttpClient;

const MATCHUP_TTL: Duration = Duration::from_secs(60);

/// One complete baseline matchup view.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchupView {
    pub matchup: Matchup,
    pub mine: RosterWeekStats,
    pub opponent: RosterWeekStats,
    pub stale: bool,
    pub odds: Vec<String>,
}

/// One contextual matchup workflow failure.
#[derive(Debug)]
pub struct MatchupError(String);

impl fmt::Display for MatchupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for MatchupError {}

fn contextual(operation: &str, error: impl fmt::Display) -> MatchupError {
    MatchupError(format!("match: {operation}: {error}"))
}

/// Acquire and render the production baseline matchup.
pub fn show(
    league_override: Option<&str>,
    requested_week: Option<i32>,
) -> Result<String, MatchupError> {
    if requested_week.is_some_and(|week| week <= 0) {
        return Err(MatchupError(
            "match: week must be positive; pass -w <week> and retry".into(),
        ));
    }
    let config = config::read().map_err(|error| contextual("read configuration", error))?;
    let league_key = league_override
        .filter(|key| !key.trim().is_empty())
        .unwrap_or(&config.current_league);
    if league_key.is_empty() {
        return Err(MatchupError(
            "match: no league selected; run b9 st -l <key> and retry".into(),
        ));
    }
    if config.current_team_key.is_empty() && league_override.is_none() {
        return Err(MatchupError(
            "match: no authenticated team stored; run b9 sync and retry".into(),
        ));
    }
    let http = Arc::new(
        HttpClient::production().map_err(|error| contextual("initialize HTTP transport", error))?,
    );
    let yahoo = Arc::new(
        YahooClient::production(http.clone())
            .map_err(|error| contextual("initialize Yahoo", error))?,
    );
    let source = YahooFantasyClient::new(yahoo);
    let mut store = Store::open().map_err(|error| contextual("open database", error))?;
    let week = requested_week.or_else(|| store.fantasy_current_week(league_key).ok().flatten());
    let scoreboard_scope = format!(
        "{}:{}",
        league_key,
        week.map_or_else(|| "current".into(), |week| week.to_string())
    );
    let (matchups, scoreboard_stale) =
        cached_or_fetch(&mut store, "match_scoreboard", &scoreboard_scope, || {
            source.scoreboard(league_key, week)
        })?;
    let team_key = if league_override.is_some() {
        source
            .team_key(league_key)
            .map_err(|error| contextual("resolve authenticated team", error))?
    } else {
        config.current_team_key
    };
    let matchup = matchups.into_iter().find(|matchup| matchup.teams.iter().any(|team| team.team_key == team_key))
        .ok_or_else(|| MatchupError("match: no matchup is scheduled for the selected week; choose another week and retry".into()))?;
    let week = matchup.week;
    let my_index = matchup
        .teams
        .iter()
        .position(|team| team.team_key == team_key)
        .expect("selected matchup contains team");
    let opponent_index = 1 - my_index;
    let mine_scope = format!("{}:{week}", matchup.teams[my_index].team_key);
    let opponent_scope = format!("{}:{week}", matchup.teams[opponent_index].team_key);
    let (mine, mine_stale) = cached_or_fetch(&mut store, "match_roster", &mine_scope, || {
        source.roster_week_stats(&matchup.teams[my_index].team_key, week)
    })?;
    let (opponent, opponent_stale) =
        cached_or_fetch(&mut store, "match_roster", &opponent_scope, || {
            source.roster_week_stats(&matchup.teams[opponent_index].team_key, week)
        })?;
    let odds = acquire_odds_context(http).unwrap_or_default();
    Ok(render_matchup(
        &MatchupView {
            matchup,
            mine,
            opponent,
            stale: scoreboard_stale || mine_stale || opponent_stale,
            odds,
        },
        detected_help_color_mode(),
    ))
}

fn cached_or_fetch<T, E, F>(
    store: &mut Store,
    dataset: &str,
    scope: &str,
    fetch: F,
) -> Result<(T, bool), MatchupError>
where
    T: Serialize + DeserializeOwned,
    E: fmt::Display,
    F: FnOnce() -> Result<T, E>,
{
    cached_or_fetch_at(store, dataset, scope, SystemTime::now(), fetch)
}

/// Reuse, refresh, or fall back to a command snapshot at an injected time.
pub fn cached_or_fetch_at<T, E, F>(
    store: &mut Store,
    dataset: &str,
    scope: &str,
    now: SystemTime,
    fetch: F,
) -> Result<(T, bool), MatchupError>
where
    T: Serialize + DeserializeOwned,
    E: fmt::Display,
    F: FnOnce() -> Result<T, E>,
{
    let previous = store
        .command_snapshot(dataset, "yahoo", scope)
        .map_err(|error| contextual("read cached data", error))?;
    if let Some(snapshot) = &previous
        && !snapshot.stale
        && now
            .duration_since(snapshot.last_successful_at)
            .unwrap_or_default()
            < MATCHUP_TTL
        && let Ok(value) = serde_json::from_str(&snapshot.payload)
    {
        return Ok((value, false));
    }
    match fetch() {
        Ok(value) => {
            let payload = serde_json::to_string(&value)
                .map_err(|error| contextual("serialize refreshed data", error))?;
            store
                .save_command_snapshot(dataset, "yahoo", scope, "v1", &payload)
                .map_err(|error| contextual("save refreshed data", error))?;
            Ok((value, false))
        }
        Err(error) => {
            if let Some(snapshot) = previous {
                let _ =
                    store.mark_command_snapshot_stale(dataset, "yahoo", scope, &error.to_string());
                let value = serde_json::from_str(&snapshot.payload)
                    .map_err(|decode| contextual("decode stale data", decode))?;
                Ok((value, true))
            } else {
                Err(contextual(
                    "refresh data",
                    format!("{error}; run b9 sync, verify connectivity, and retry"),
                ))
            }
        }
    }
}

/// Render a deterministic baseline matchup surface.
pub fn render_matchup(view: &MatchupView, mode: HelpColorMode) -> String {
    let mine = &view.matchup.teams[view
        .matchup
        .teams
        .iter()
        .position(|team| team.team_key == view.mine.team_key)
        .unwrap_or(0)];
    let opponent = &view.matchup.teams[view
        .matchup
        .teams
        .iter()
        .position(|team| team.team_key == view.opponent.team_key)
        .unwrap_or(1)];
    let mut output = String::new();
    output.push_str(&title(&format!("MATCHUP WEEK {}", view.matchup.week), mode));
    output.push('\n');
    output.push_str(&format!(
        "{}  {}–{}–{}    {}  {}–{}–{}\n",
        mine.name,
        mine.wins,
        mine.losses,
        mine.ties,
        opponent.name,
        opponent.wins,
        opponent.losses,
        opponent.ties
    ));
    if view.stale {
        output.push_str("STALE — showing the last complete Yahoo matchup snapshot.\n");
    }
    render_categories(&mut output, mine, opponent, mode);
    render_players(
        &mut output,
        "HITTERS",
        &view.mine.players,
        &view.opponent.players,
        "B",
        mode,
    );
    if !view.odds.is_empty() {
        output.push('\n');
        output.push_str(&section("ODDS", mode));
        output.push('\n');
        for line in &view.odds {
            output.push_str("  ");
            output.push_str(line);
            output.push('\n');
        }
    }
    render_players(
        &mut output,
        "PITCHERS",
        &view.mine.players,
        &view.opponent.players,
        "P",
        mode,
    );
    output.push_str(&format!(
        "Games: {} complete / {} live / {} remaining\n",
        mine.completed_games + opponent.completed_games,
        mine.live_games + opponent.live_games,
        mine.remaining_games + opponent.remaining_games
    ));
    output
}

fn acquire_odds_context(http: Arc<HttpClient>) -> Result<Vec<String>, MatchupError> {
    let now = SystemTime::now();
    let date = utc_date(now)?;
    let schedule = crate::providers::mlb::MlbClient::production(http.clone())
        .fetch_schedule(&date)
        .map_err(|error| contextual("refresh MLB schedule", error))?;
    let lines = crate::providers::espn::EspnClient::production(http)
        .fetch_game_lines(now)
        .map_err(|error| contextual("refresh ESPN odds", error))?;
    let mut output = Vec::new();
    for game in schedule {
        if let Some(line) = lines.games.iter().find(|line| {
            normalized_team(&line.home_team) == normalized_team(&game.home_team_name)
                && normalized_team(&line.away_team) == normalized_team(&game.away_team_name)
        }) && line.quoted
        {
            let (away, home) = normalized_probabilities(line.away_moneyline, line.home_moneyline);
            output.push(format!(
                "{} {:.0}% @ {} {:.0}%",
                line.away_team,
                away * 100.0,
                line.home_team,
                home * 100.0
            ));
        }
    }
    Ok(output)
}

fn normalized_team(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalized_probabilities(away: i64, home: i64) -> (f64, f64) {
    let implied = |line: i64| {
        if line > 0 {
            100.0 / (line as f64 + 100.0)
        } else if line < 0 {
            (-line) as f64 / ((-line) as f64 + 100.0)
        } else {
            0.0
        }
    };
    let away = implied(away);
    let home = implied(home);
    let total = away + home;
    if total == 0.0 {
        (0.0, 0.0)
    } else {
        (away / total, home / total)
    }
}

fn utc_date(time: SystemTime) -> Result<String, MatchupError> {
    let days = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| MatchupError("match: system clock precedes the Unix epoch".into()))?
        .as_secs()
        / 86_400;
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

fn render_categories(
    output: &mut String,
    mine: &MatchupTeam,
    opponent: &MatchupTeam,
    mode: HelpColorMode,
) {
    output.push('\n');
    output.push_str(&section("CATEGORIES", mode));
    output.push('\n');
    let mut keys = mine
        .stats
        .keys()
        .chain(opponent.stats.keys())
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    if keys.is_empty() {
        output.push_str("  Category totals unavailable\n");
        return;
    }
    for key in keys {
        output.push_str(&format!(
            "  {key:<8} {:>8}  {:>8}\n",
            mine.stats.get(key).map(String::as_str).unwrap_or("—"),
            opponent.stats.get(key).map(String::as_str).unwrap_or("—")
        ));
    }
}

fn render_players(
    output: &mut String,
    heading: &str,
    mine: &[PlayerWeekStats],
    opponent: &[PlayerWeekStats],
    role: &str,
    mode: HelpColorMode,
) {
    output.push('\n');
    output.push_str(&section(heading, mode));
    output.push('\n');
    let left = mine
        .iter()
        .filter(|player| player.position_type == role)
        .collect::<Vec<_>>();
    let right = opponent
        .iter()
        .filter(|player| player.position_type == role)
        .collect::<Vec<_>>();
    let rows = left.len().max(right.len());
    for index in 0..rows {
        let left = left
            .get(index)
            .map(|player| {
                format!(
                    "{:<3} {:<22}",
                    player.slot_position.to_string(),
                    player.name
                )
            })
            .unwrap_or_else(|| " ".repeat(25));
        let right = right
            .get(index)
            .map(|player| format!("{:<3} {}", player.slot_position.to_string(), player.name))
            .unwrap_or_default();
        output.push_str(&format!("  {left} | {right}\n"));
    }
}
