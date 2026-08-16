//! Lazy Yahoo matchup acquisition, durable fallback, view assembly, and rendering.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde::{Serialize, de::DeserializeOwned};

use crate::advisory::{
    AdvisoryResponse, build_advisory_context, compute_category_gaps, compute_lineup_candidates,
    compute_risk_alerts, compute_roster_moves, compute_slot_gaps, grounded_response,
};
use crate::advisory_credentials::{load_credential, selected_provider};
use crate::config;
use crate::domain::{
    Matchup, MatchupTeam, PlayerWeekStats, Position, RosterWeekStats, StoredFantasyPlayer,
    is_valid_iso_date,
};
use crate::providers::advisory::{AdvisoryClient, AdvisoryProvider};
use crate::providers::mlb::{BulkHittingSplit, BulkPitchingSplit, MlbClient};
use crate::providers::yahoo::YahooClient;
use crate::providers::yahoo_fantasy::{YahooFantasyClient, YahooFantasySource};
use crate::store::Store;
use crate::terminal::{HelpColorMode, detected_help_color_mode, section, title};
use crate::transport::HttpClient;

const MATCHUP_TTL: Duration = Duration::from_secs(60);

/// Parsed matchup period and advisory selection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MatchupOptions {
    pub week: Option<i32>,
    pub weekly: bool,
    pub day: Option<String>,
    pub advise: bool,
}

impl MatchupOptions {
    /// Validate selector combinations before provider or store access.
    pub fn validate(&self) -> Result<(), MatchupError> {
        if self.week.is_some_and(|week| week <= 0) {
            return Err(MatchupError(
                "match: week must be positive; pass -w <week> and retry".into(),
            ));
        }
        if self.day.is_some() && (self.week.is_some() || self.weekly) {
            return Err(MatchupError(
                "match: --day cannot be combined with --week or --weekly; choose one period and retry"
                    .into(),
            ));
        }
        if self
            .day
            .as_deref()
            .is_some_and(|day| !is_valid_iso_date(day))
        {
            return Err(MatchupError(
                "match: day must use YYYY-MM-DD; correct the date and retry".into(),
            ));
        }
        Ok(())
    }
}

/// One complete baseline matchup view.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchupView {
    pub matchup: Matchup,
    pub mine: RosterWeekStats,
    pub opponent: RosterWeekStats,
    pub stale: bool,
    pub odds: Vec<String>,
}

/// A local-only fallback when no compatible Yahoo scoreboard exists.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalMatchupView {
    pub team_name: String,
    pub players: Vec<PlayerWeekStats>,
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
    show_with_options(
        league_override,
        MatchupOptions {
            week: requested_week,
            ..MatchupOptions::default()
        },
    )
}

/// Acquire and render a matchup using the fully parsed command options.
pub fn show_with_options(
    league_override: Option<&str>,
    options: MatchupOptions,
) -> Result<String, MatchupError> {
    options.validate()?;
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
    let current_week = store
        .fantasy_current_week(league_key)
        .map_err(|error| contextual("read current matchup week", error))?;
    let week = match (&options.day, options.week, options.weekly) {
        (_, Some(week), _) => Some(week),
        (_, None, true) | (None, None, false) => current_week,
        (Some(day), None, false) => Some(resolve_day_week(
            &mut store,
            &source,
            league_key,
            current_week,
            day,
        )?),
    };
    let scoreboard_scope = format!(
        "{}:{}",
        league_key,
        week.map_or_else(|| "current".into(), |week| week.to_string())
    );
    let team_key = if league_override.is_some() {
        source
            .team_key(league_key)
            .map_err(|error| contextual("resolve authenticated team", error))?
    } else {
        config.current_team_key.clone()
    };
    let (matchups, scoreboard_stale) =
        match cached_or_fetch(&mut store, "match_scoreboard", &scoreboard_scope, || {
            source.scoreboard(league_key, week)
        }) {
            Ok(result) => result,
            Err(scoreboard_error) => {
                return match local_matchup_view(&store, league_key, &team_key) {
                    Ok(view) => Ok(render_local_matchup(&view, detected_help_color_mode())),
                    Err(_) => Err(scoreboard_error),
                };
            }
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
    let (mut mine, mine_stale) = cached_or_fetch(&mut store, "match_roster", &mine_scope, || {
        source.roster_week_stats(&matchup.teams[my_index].team_key, week)
    })?;
    let (mut opponent, opponent_stale) =
        cached_or_fetch(&mut store, "match_roster", &opponent_scope, || {
            source.roster_week_stats(&matchup.teams[opponent_index].team_key, week)
        })?;
    let daily_date = if options.week.is_none() && !options.weekly {
        Some(options.day.clone().unwrap_or(utc_date(SystemTime::now())?))
    } else {
        None
    };
    if let Some(day) = &daily_date {
        apply_daily_stats(
            &store,
            league_key,
            &mut mine,
            &mut opponent,
            day,
            http.clone(),
        )?;
    }
    let odds = acquire_odds_context(http.clone()).unwrap_or_default();
    let stale = scoreboard_stale || mine_stale || opponent_stale;
    let view = MatchupView {
        matchup,
        mine,
        opponent,
        stale,
        odds,
    };
    let mut output = render_matchup(&view, detected_help_color_mode());
    if let Some(day) = daily_date {
        output = format!("DAY {day}\n{output}");
    }
    if options.advise {
        let response = acquire_advisory(&store, league_key, &config, &view, http)?;
        render_advisory_response(&mut output, &response, detected_help_color_mode());
    }
    Ok(output)
}

fn apply_daily_stats(
    store: &Store,
    league_key: &str,
    mine: &mut RosterWeekStats,
    opponent: &mut RosterWeekStats,
    day: &str,
    http: Arc<HttpClient>,
) -> Result<(), MatchupError> {
    let season = day[..4].parse::<i64>().map_err(|_| {
        MatchupError("match: day must include a valid year; correct the date and retry".into())
    })?;
    let mlb = MlbClient::production(http);
    let hitting = mlb
        .fetch_hitting_stats_by_date_range(season, day, day)
        .map_err(|error| contextual("refresh daily MLB hitting stats", error))?;
    let pitching = mlb
        .fetch_pitching_stats_by_date_range(season, day, day)
        .map_err(|error| contextual("refresh daily MLB pitching stats", error))?;
    let identities = store
        .fantasy_players(league_key)
        .map_err(|error| contextual("read daily player identities", error))?
        .into_iter()
        .filter_map(|player| player.yahoo_player_id.zip(player.mlbam_id))
        .collect::<HashMap<_, _>>();
    apply_daily_roster(&mut mine.players, &identities, &hitting, &pitching);
    apply_daily_roster(&mut opponent.players, &identities, &hitting, &pitching);
    Ok(())
}

fn apply_daily_roster(
    players: &mut [PlayerWeekStats],
    identities: &HashMap<i64, i64>,
    hitting: &[BulkHittingSplit],
    pitching: &[BulkPitchingSplit],
) {
    for player in players {
        let Some(mlbam_id) = identities.get(&player.yahoo_player_id) else {
            continue;
        };
        if let Some(split) = hitting
            .iter()
            .find(|split| split.player.person_id == *mlbam_id)
        {
            player.hab = split.stat.at_bats.to_string();
            player.runs = split.stat.runs as i32;
            player.home_runs = split.stat.home_runs as i32;
            player.runs_batted_in = split.stat.rbi as i32;
            player.stolen_bases = split.stat.stolen_bases as i32;
            player.batting_average = split.stat.average.clone();
        }
        if let Some(split) = pitching
            .iter()
            .find(|split| split.player.person_id == *mlbam_id)
        {
            player.innings_pitched = split.stat.innings_pitched.clone();
            player.wins = split.stat.wins as i32;
            player.saves = split.stat.saves as i32;
            player.strikeouts = split.stat.strikeouts as i32;
            player.earned_run_average = split.stat.era.clone();
            player.whip = split.stat.whip.clone();
        }
    }
}

fn resolve_day_week(
    store: &mut Store,
    source: &impl YahooFantasySource,
    league_key: &str,
    current_week: Option<i32>,
    day: &str,
) -> Result<i32, MatchupError> {
    let current_week = current_week.ok_or_else(|| {
        MatchupError("match: current matchup week is unavailable; run b9 sync and retry".into())
    })?;
    for week in 1..=current_week {
        let scope = format!("{league_key}:{week}");
        let (matchups, _) = cached_or_fetch(store, "match_scoreboard", &scope, || {
            source.scoreboard(league_key, Some(week))
        })?;
        if matchups
            .iter()
            .any(|matchup| matchup.week_start.as_str() <= day && day <= matchup.week_end.as_str())
        {
            return Ok(week);
        }
    }
    Err(MatchupError(
        "match: day is outside available Yahoo matchup weeks; choose a current-matchup date and retry"
            .into(),
    ))
}

fn acquire_advisory(
    store: &Store,
    league_key: &str,
    config: &config::Config,
    view: &MatchupView,
    http: Arc<HttpClient>,
) -> Result<AdvisoryResponse, MatchupError> {
    if view.stale {
        return Err(MatchupError(
            "match: advisory requires a fresh complete matchup; retry after Yahoo refresh succeeds"
                .into(),
        ));
    }
    let provider_name = selected_provider(config).ok_or_else(|| {
        MatchupError("match: advisory provider is not configured; configure lm and retry".into())
    })?;
    let credential = load_credential(provider_name)
        .map_err(|error| contextual("read advisory credential", error))?
        .ok_or_else(|| {
            MatchupError("match: advisory credential is unavailable; configure lm and retry".into())
        })?;
    let provider = AdvisoryProvider::parse(provider_name)
        .map_err(|error| contextual("select advisory provider", error))?;
    let categories = store
        .fantasy_categories(league_key)
        .map_err(|error| contextual("read matchup categories", error))?
        .into_iter()
        .map(|category| category.abbreviation)
        .collect::<Vec<_>>();
    let gaps = compute_category_gaps(
        &view.matchup.teams[0],
        &view.matchup.teams[1],
        &categories,
        &config.strategy_punts,
    );
    let lineup_candidates = compute_lineup_candidates(&view.mine);
    let free_agent_values = store
        .fantasy_players(league_key)
        .map_err(|error| contextual("read free-agent candidates", error))?
        .into_iter()
        .filter(|player| player.owner.is_none())
        .flat_map(|player| free_agent_values(&player, &categories))
        .collect::<Vec<_>>();
    let roster_moves = compute_roster_moves(&gaps, &free_agent_values);
    let positions = store
        .fantasy_positions(league_key)
        .map_err(|error| contextual("read matchup positions", error))?
        .into_iter()
        .map(|position| (Position::from(position.position.as_str()), position.count))
        .collect::<Vec<_>>();
    let slot_gaps = compute_slot_gaps(&positions, &view.mine);
    let risks = compute_risk_alerts(&view.mine);
    let context = build_advisory_context(&lineup_candidates, &roster_moves);
    let response = AdvisoryClient::new(http)
        .complete(provider, &config.advisory_model, &credential, &context)
        .map_err(|error| contextual("request advisory", error))?;
    let mut response = grounded_response(&context, response);
    response.confirmations.extend(
        gaps.iter()
            .filter(|gap| gap.leading && !gap.punted)
            .map(|gap| format!("Leading {}", gap.category)),
    );
    response.confirmations.extend(
        slot_gaps
            .iter()
            .map(|gap| format!("Uncovered {} slot", gap.slot)),
    );
    response.risks.extend(
        risks
            .into_iter()
            .map(|risk| format!("{}: {}", risk.player_name, risk.reason)),
    );
    Ok(response)
}

fn free_agent_values(
    player: &StoredFantasyPlayer,
    categories: &[String],
) -> Vec<crate::domain::FreeAgentCategoryValue> {
    categories
        .iter()
        .filter_map(|category| {
            let value = match category.as_str() {
                "R" => player.batting[2],
                "HR" => player.batting[3],
                "RBI" => player.batting[4],
                "SB" => player.batting[5],
                "AVG" => player.batting[6],
                "W" => player.pitching[2],
                "SV" => player.pitching[3],
                "K" => player.pitching[4],
                "ERA" => player.pitching[5],
                "WHIP" => player.pitching[6],
                _ => return None,
            };
            Some(crate::domain::FreeAgentCategoryValue {
                player_name: player.name.clone(),
                category: category.clone(),
                value,
            })
        })
        .collect()
}

/// Render only a grounded advisory response under the full-matchup surface.
pub fn render_advisory_response(
    output: &mut String,
    response: &AdvisoryResponse,
    mode: HelpColorMode,
) {
    output.push('\n');
    output.push_str(&section("ADVICE", mode));
    output.push('\n');
    for confirmation in &response.confirmations {
        output.push_str(&format!("  {confirmation}\n"));
    }
    for action in &response.urgent {
        output.push_str(&format!("  Urgent: {}\n", action.summary));
    }
    for action in &response.overnight {
        output.push_str(&format!("  Overnight: {}\n", action.summary));
    }
    for risk in &response.risks {
        output.push_str(&format!("  Risk: {risk}\n"));
    }
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

/// Render the deliberately limited fallback without matchup or advisory claims.
pub fn render_local_matchup(view: &LocalMatchupView, mode: HelpColorMode) -> String {
    let mut output = format!(
        "{}\nLOCAL ROSTER — Yahoo matchup data is unavailable.\n",
        title(&view.team_name, mode)
    );
    render_local_players(&mut output, "HITTERS", &view.players, "B", mode);
    render_local_players(&mut output, "PITCHERS", &view.players, "P", mode);
    output
}

fn local_matchup_view(
    store: &Store,
    league_key: &str,
    team_key: &str,
) -> Result<LocalMatchupView, MatchupError> {
    let team = store
        .fantasy_teams(league_key)
        .map_err(|error| contextual("read local teams", error))?
        .into_iter()
        .find(|team| team.team_key == team_key)
        .ok_or_else(|| {
            MatchupError("match: no local roster is available; run b9 sync and retry".into())
        })?;
    let players = store
        .fantasy_players(league_key)
        .map_err(|error| contextual("read local roster", error))?
        .into_iter()
        .filter(|player| player.owner.as_deref() == Some(team.name.as_str()))
        .map(local_player_week_stats)
        .collect();
    Ok(LocalMatchupView {
        team_name: team.name,
        players,
    })
}

fn local_player_week_stats(player: StoredFantasyPlayer) -> PlayerWeekStats {
    let eligible_positions = player
        .positions
        .split(',')
        .filter(|position| !position.is_empty())
        .map(Position::from)
        .collect();
    PlayerWeekStats {
        yahoo_player_id: player.yahoo_player_id.unwrap_or_default(),
        name: player.name,
        team: player.team,
        position_type: player.role.clone(),
        slot_position: player
            .slot
            .as_deref()
            .map_or(Position::Bench, Position::from),
        eligible_positions,
        injury_status: player.status,
        hab: player.batting[0].round().to_string(),
        runs: player.batting[2].round() as i32,
        home_runs: player.batting[3].round() as i32,
        runs_batted_in: player.batting[4].round() as i32,
        stolen_bases: player.batting[5].round() as i32,
        batting_average: format!("{:.3}", player.batting[6]),
        innings_pitched: format!("{:.1}", player.pitching[0]),
        wins: player.pitching[2].round() as i32,
        saves: player.pitching[3].round() as i32,
        strikeouts: player.pitching[4].round() as i32,
        earned_run_average: format!("{:.2}", player.pitching[5]),
        whip: format!("{:.2}", player.pitching[6]),
    }
}

fn render_local_players(
    output: &mut String,
    heading: &str,
    players: &[PlayerWeekStats],
    role: &str,
    mode: HelpColorMode,
) {
    output.push('\n');
    output.push_str(&section(heading, mode));
    output.push('\n');
    for player in players.iter().filter(|player| player.position_type == role) {
        output.push_str(&format!(
            "  {:<3} {} ({})\n",
            player.slot_position, player.name, player.team
        ));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::mlb::{BulkHittingSplit, BulkPitchingSplit, HittingStats, PitchingStats};

    #[test]
    fn daily_projection_replaces_only_matched_player_stats() {
        let mut players = vec![PlayerWeekStats {
            yahoo_player_id: 7,
            name: "Ada".into(),
            team: "B9".into(),
            position_type: "B".into(),
            slot_position: Position::Outfield,
            eligible_positions: vec![],
            injury_status: String::new(),
            hab: String::new(),
            runs: 0,
            home_runs: 0,
            runs_batted_in: 0,
            stolen_bases: 0,
            batting_average: String::new(),
            innings_pitched: String::new(),
            wins: 0,
            saves: 0,
            strikeouts: 0,
            earned_run_average: String::new(),
            whip: String::new(),
        }];
        let hitting = vec![BulkHittingSplit {
            player: crate::providers::mlb::BulkPlayer {
                person_id: 42,
                full_name: String::new(),
            },
            team: Default::default(),
            position: Default::default(),
            stat: HittingStats {
                at_bats: 4,
                runs: 2,
                home_runs: 1,
                rbi: 3,
                stolen_bases: 1,
                average: ".500".into(),
                ..Default::default()
            },
        }];
        let pitching = vec![BulkPitchingSplit {
            player: crate::providers::mlb::BulkPlayer {
                person_id: 42,
                full_name: String::new(),
            },
            team: Default::default(),
            position: Default::default(),
            stat: PitchingStats {
                innings_pitched: "6.0".into(),
                wins: 1,
                strikeouts: 8,
                era: "1.50".into(),
                whip: "0.83".into(),
                ..Default::default()
            },
        }];
        apply_daily_roster(&mut players, &HashMap::from([(7, 42)]), &hitting, &pitching);
        assert_eq!(
            (
                players[0].hab.as_str(),
                players[0].home_runs,
                players[0].strikeouts
            ),
            ("4", 1, 8)
        );
    }
}
