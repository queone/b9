//! Lazy Yahoo matchup acquisition, durable fallback, view assembly, and rendering.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

use serde::{Serialize, de::DeserializeOwned};

use crate::config;
use crate::domain::{
    Matchup, MatchupTeam, PlayerWeekStats, Position, RosterWeekStats, StoredFantasyPlayer,
    clean_fantasy_team_name, is_valid_iso_date,
};
use crate::player_display::render_players as render_roster_players;
use crate::providers::mlb::{BulkHittingSplit, BulkPitchingSplit, MlbClient, ScheduleGame};
use crate::providers::yahoo_fantasy::YahooFantasySource;
use crate::providers::yahoo_public::{RedzoneFeed, YahooPublicClient, league_id_from_key};
use crate::store::{Store, StoredFantasyTeam};
use crate::terminal::{
    HelpColorMode, available, detected_help_color_mode, dim, good, injury_status, lineup_indicator,
    table_heading, visible_width, warning,
};
use crate::transport::HttpClient;

const MATCHUP_TTL: Duration = Duration::from_secs(60);

/// Parsed matchup period selection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MatchupOptions {
    pub week: Option<i32>,
    pub weekly: bool,
    pub day: Option<String>,
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
        if self.day.as_deref().is_some_and(parse_short_day_invalid) {
            return Err(MatchupError(
                "match: day must use MMM-DD; correct the date and retry".into(),
            ));
        }
        Ok(())
    }
}

fn parse_short_day_invalid(day: &str) -> bool {
    short_day_parts(day).is_none()
}

fn short_day_parts(day: &str) -> Option<(u32, u32)> {
    let (month, day) = day.split_once('-')?;
    if month.len() != 3 || day.len() != 2 || !day.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let month = match month.to_ascii_lowercase().as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    };
    Some((month, day.parse().ok()?))
}

fn season_day(day: &str, season: i64) -> Result<String, MatchupError> {
    let (month, day) = short_day_parts(day).ok_or_else(|| {
        MatchupError("match: day must use MMM-DD; correct the date and retry".into())
    })?;
    let resolved = format!("{season:04}-{month:02}-{day:02}");
    if !is_valid_iso_date(&resolved) {
        return Err(MatchupError(
            "match: day is not valid in the active season; correct the date and retry".into(),
        ));
    }
    Ok(resolved)
}

/// One complete baseline matchup view.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchupView {
    pub matchup: Matchup,
    pub mine: RosterWeekStats,
    pub opponent: RosterWeekStats,
    pub teams: Vec<StoredFantasyTeam>,
    pub stale: bool,
    pub odds: Vec<MatchupOdds>,
}

/// One probable-pitcher odds row assigned to a matchup side.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchupOdds {
    pub mine: bool,
    pub line: String,
}

/// A local-only fallback when no compatible Yahoo scoreboard exists.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalMatchupView {
    pub team_name: String,
    pub players: Vec<StoredFantasyPlayer>,
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
    show_with_team_options(league_override, None, options)
}

/// Acquire and render a matchup, with an optional explicit team selector.
///
/// The team argument resolves "my team" by name/manager substring, matching
/// `r <team>`'s existing lookup. When it resolves, it's persisted to
/// `config.current_team_key` so it is needed only once, not on every invocation.
pub fn show_with_team_options(
    league_override: Option<&str>,
    team_override: Option<&str>,
    options: MatchupOptions,
) -> Result<String, MatchupError> {
    options.validate()?;
    let mut config = config::read().map_err(|error| contextual("read configuration", error))?;
    let league_key = league_override
        .filter(|key| !key.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| config.current_league.clone());
    if league_key.is_empty() {
        return Err(MatchupError(
            "match: no league selected; run b9 st -l <key> and retry".into(),
        ));
    }
    let mut store = Store::open().map_err(|error| contextual("open database", error))?;
    let resolved_team_override = match team_override.filter(|value| !value.trim().is_empty()) {
        Some(query) => {
            let teams = store
                .fantasy_teams(&league_key)
                .map_err(|error| contextual("read teams for team selection", error))?;
            let (resolved, changed) = resolve_team_override(&teams, &mut config, query)?;
            if changed {
                config::write(&config).map_err(|error| contextual("save team selection", error))?;
            }
            Some(resolved)
        }
        None => None,
    };

    let http = Arc::new(
        HttpClient::production().map_err(|error| contextual("initialize HTTP transport", error))?,
    );
    let effective_team_key = resolved_team_override
        .clone()
        .or_else(|| (!config.current_team_key.is_empty()).then(|| config.current_team_key.clone()));

    let source = YahooPublicClient::shared(http.clone());
    if options.day.is_none() && options.week.is_none() {
        let public_league_id = league_id_from_key(&league_key)
            .map_err(|error| contextual("resolve public league id", error))?;
        let public_client = YahooPublicClient::production()
            .map_err(|error| contextual("initialize public feed client", error))?;
        return show_weekly_matchup(
            &mut store,
            &league_key,
            effective_team_key,
            &public_client,
            &public_league_id,
            options.weekly,
            (http, SystemTime::now()),
        );
    }

    if effective_team_key.is_none() {
        return Err(MatchupError(
            "match: no primary team selected; run b9 sync -T <key-or-name> and retry".into(),
        ));
    }
    let current_week = store
        .fantasy_current_week(&league_key)
        .map_err(|error| contextual("read current matchup week", error))?;
    if options
        .week
        .zip(current_week)
        .is_some_and(|(requested, current)| requested > current)
    {
        return Err(MatchupError(
            "match: selected week has not started; choose the current week or an earlier week"
                .into(),
        ));
    }
    let resolved_day = if let Some(day) = &options.day {
        let season = store
            .fantasy_season(&league_key)
            .map_err(|error| contextual("read active league season", error))?
            .ok_or_else(|| {
                MatchupError(
                    "match: active league season is unavailable; run b9 sync and retry".into(),
                )
            })?;
        Some(season_day(day, season)?)
    } else {
        None
    };
    let week = match (&options.day, options.week, options.weekly) {
        (_, Some(week), _) => Some(week),
        (_, None, true) | (None, None, false) => current_week,
        (Some(_), None, false) => Some(resolve_day_week(
            &mut store,
            &source,
            &league_key,
            current_week,
            resolved_day.as_deref().expect("selected day resolved"),
        )?),
    };
    let scoreboard_scope = format!(
        "{}:{}",
        league_key,
        week.map_or_else(|| "current".into(), |week| week.to_string())
    );
    let historical = week
        .zip(current_week)
        .is_some_and(|(selected, current)| selected < current);
    let team_key = effective_team_key.expect("primary team checked above");
    let (matchups, scoreboard_stale) = match persisted_or_fetch(
        &mut store,
        "match_scoreboard",
        &scoreboard_scope,
        historical,
        || source.scoreboard(&league_key, week),
    ) {
        Ok(result) => result,
        Err(scoreboard_error) => {
            return match local_matchup_view(&store, &league_key, &team_key) {
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
    let roster_dataset = if resolved_day.is_some() {
        "match_roster_day"
    } else {
        "match_roster"
    };
    let roster_period = resolved_day
        .as_deref()
        .map_or_else(|| week.to_string(), str::to_owned);
    let mine_scope = format!("{}:{roster_period}", matchup.teams[my_index].team_key);
    let opponent_scope = format!("{}:{roster_period}", matchup.teams[opponent_index].team_key);
    let (mut mine, mine_stale) =
        persisted_roster_or_fetch(&mut store, roster_dataset, &mine_scope, historical, || {
            if let Some(day) = resolved_day.as_deref() {
                source.roster_day_stats(&matchup.teams[my_index].team_key, week, day)
            } else {
                source.roster_week_stats(&matchup.teams[my_index].team_key, week)
            }
        })?;
    let (mut opponent, opponent_stale) = persisted_roster_or_fetch(
        &mut store,
        roster_dataset,
        &opponent_scope,
        historical,
        || {
            if let Some(day) = resolved_day.as_deref() {
                source.roster_day_stats(&matchup.teams[opponent_index].team_key, week, day)
            } else {
                source.roster_week_stats(&matchup.teams[opponent_index].team_key, week)
            }
        },
    )?;
    enrich_historical_roster(&store, &mut mine)?;
    enrich_historical_roster(&store, &mut opponent)?;
    let daily_date = if options.week.is_none() && !options.weekly {
        Some(resolved_day.unwrap_or(utc_date(SystemTime::now())?))
    } else {
        None
    };
    if let Some(day) = &daily_date {
        apply_daily_stats(&store, &mut mine, &mut opponent, day, http.clone())?;
    }
    apply_roster_statuses(&store, &league_key, &mut mine, &mut opponent)?;
    let odds = acquire_odds_context(&mut store, http.clone(), &mine, &opponent).unwrap_or_default();
    let stale = scoreboard_stale || mine_stale || opponent_stale;
    let teams = store
        .fantasy_teams(&league_key)
        .map_err(|error| contextual("read matchup team context", error))?;
    let view = MatchupView {
        matchup,
        mine,
        opponent,
        teams,
        stale,
        odds,
    };
    let mut output = render_matchup(&view, detected_help_color_mode());
    if let Some(day) = daily_date {
        output = format!("DAY {day}\n{output}");
    }
    Ok(output)
}

fn apply_daily_stats(
    store: &Store,
    mine: &mut RosterWeekStats,
    opponent: &mut RosterWeekStats,
    day: &str,
    http: Arc<HttpClient>,
) -> Result<(), MatchupError> {
    let season = day[..4].parse::<i64>().map_err(|_| {
        MatchupError("match: day must include a valid year; correct the date and retry".into())
    })?;
    let yahoo_player_ids = mine
        .players
        .iter()
        .chain(&opponent.players)
        .map(|player| player.yahoo_player_id)
        .collect::<Vec<_>>();
    let identities = store
        .mlb_identities_for_yahoo_players(&yahoo_player_ids)
        .map_err(|error| contextual("read daily player identities", error))?;
    let identities = required_mlb_identities(identities, mine, opponent)?;
    let hitting_http = http.clone();
    let day_for_hitting = day.to_owned();
    let day_for_pitching = day.to_owned();
    let (hitting, pitching) = parallel_pair(
        move || {
            MlbClient::production(hitting_http).fetch_hitting_stats_by_date_range(
                season,
                &day_for_hitting,
                &day_for_hitting,
            )
        },
        move || {
            MlbClient::production(http).fetch_pitching_stats_by_date_range(
                season,
                &day_for_pitching,
                &day_for_pitching,
            )
        },
    );
    let hitting = hitting
        .map_err(|_| MatchupError("match: refresh daily MLB hitting stats: worker failed".into()))?
        .map_err(|error| contextual("refresh daily MLB hitting stats", error))?;
    let pitching = pitching
        .map_err(|_| MatchupError("match: refresh daily MLB pitching stats: worker failed".into()))?
        .map_err(|error| contextual("refresh daily MLB pitching stats", error))?;
    apply_daily_roster(&mut mine.players, &identities, &hitting, &pitching);
    apply_daily_roster(&mut opponent.players, &identities, &hitting, &pitching);
    Ok(())
}

fn enrich_historical_roster(
    store: &Store,
    roster: &mut RosterWeekStats,
) -> Result<(), MatchupError> {
    let ids = roster
        .players
        .iter()
        .map(|player| player.yahoo_player_id)
        .collect::<Vec<_>>();
    let metadata = store
        .yahoo_player_metadata(&ids)
        .map_err(|error| contextual("read historical player metadata", error))?;
    for player in &mut roster.players {
        let Some((_, name, team, role)) = metadata
            .iter()
            .find(|(id, _, _, _)| *id == player.yahoo_player_id)
        else {
            continue;
        };
        if player.name.is_empty() {
            player.name.clone_from(name);
        }
        if player.team.is_empty() {
            player.team.clone_from(team);
        }
        if player.position_type.is_empty() {
            player.position_type.clone_from(role);
        }
    }
    Ok(())
}

fn parallel_pair<A, B, Left, Right>(
    left: Left,
    right: Right,
) -> (thread::Result<A>, thread::Result<B>)
where
    A: Send,
    B: Send,
    Left: FnOnce() -> A + Send,
    Right: FnOnce() -> B + Send,
{
    thread::scope(|scope| {
        let left = scope.spawn(left);
        let right = scope.spawn(right);
        (left.join(), right.join())
    })
}

fn required_mlb_identities(
    identities: Vec<(i64, i64)>,
    mine: &RosterWeekStats,
    opponent: &RosterWeekStats,
) -> Result<HashMap<i64, i64>, MatchupError> {
    let identities = identities.into_iter().collect::<HashMap<_, _>>();
    let missing = mine
        .players
        .iter()
        .chain(&opponent.players)
        .filter(|player| !identities.contains_key(&player.yahoo_player_id))
        .map(|player| player.name.as_str())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(MatchupError(format!(
            "match: daily overlay requires reconciled MLB identities; run b9 sync and retry (unresolved: {})",
            missing.join(", ")
        )));
    }
    Ok(identities)
}

fn apply_daily_roster(
    players: &mut [PlayerWeekStats],
    identities: &HashMap<i64, i64>,
    hitting: &[BulkHittingSplit],
    pitching: &[BulkPitchingSplit],
) {
    for player in players {
        player.hab = "0-0".into();
        player.runs = 0;
        player.home_runs = 0;
        player.runs_batted_in = 0;
        player.stolen_bases = 0;
        player.batting_average = "0.000".into();
        player.innings_pitched = "0.0".into();
        player.wins = 0;
        player.saves = 0;
        player.strikeouts = 0;
        player.earned_run_average = "0.00".into();
        player.whip = "0.00".into();
        let Some(mlbam_id) = identities.get(&player.yahoo_player_id) else {
            continue;
        };
        if let Some(split) = hitting
            .iter()
            .find(|split| split.player.person_id == *mlbam_id)
        {
            player.hab = format!("{}-{}", split.stat.hits, split.stat.at_bats);
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

fn apply_roster_statuses(
    store: &Store,
    league_key: &str,
    mine: &mut RosterWeekStats,
    opponent: &mut RosterWeekStats,
) -> Result<(), MatchupError> {
    let identities = store
        .fantasy_players(league_key)
        .map_err(|error| contextual("read roster status identities", error))?;
    let roster_ids = mine
        .players
        .iter()
        .chain(&opponent.players)
        .map(|player| player.yahoo_player_id)
        .collect::<HashSet<_>>();
    let mut roster_players = identities
        .iter()
        .filter(|player| {
            player
                .yahoo_player_id
                .is_some_and(|id| roster_ids.contains(&id))
        })
        .cloned()
        .collect::<Vec<_>>();
    crate::player_commands::populate_game_statuses(&mut roster_players, &identities);
    apply_resolved_roster_statuses(&mut mine.players, &roster_players);
    apply_resolved_roster_statuses(&mut opponent.players, &roster_players);
    Ok(())
}

fn apply_resolved_roster_statuses(
    players: &mut [PlayerWeekStats],
    resolved: &[StoredFantasyPlayer],
) {
    for player in players {
        let Some(status) = resolved
            .iter()
            .find(|stored| stored.yahoo_player_id == Some(player.yahoo_player_id))
        else {
            continue;
        };
        if !status.status.is_empty() {
            player.injury_status.clone_from(&status.status);
        } else if !status.game_status.is_empty() {
            player.injury_status.clone_from(&status.game_status);
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

/// Resolve a team by case-insensitive name/manager substring, matching
/// `r <team>`'s existing lookup exactly (same ambiguity/no-match errors).
fn select_matchup_team<'a>(
    teams: &'a [StoredFantasyTeam],
    query: &str,
) -> Result<&'a StoredFantasyTeam, MatchupError> {
    let needle = query.to_lowercase();
    let matches = teams
        .iter()
        .filter(|team| {
            team.name.to_lowercase().contains(&needle)
                || team.manager_name.to_lowercase().contains(&needle)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [team] => Ok(*team),
        [] => Err(MatchupError("match: no team matches the query".into())),
        _ => Err(MatchupError(format!(
            "match: query is ambiguous; matches: {}",
            matches
                .iter()
                .map(|team| team.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Resolve a `[team]` argument against known teams and stage the config
/// mutation in memory. Only the caller touches `config::write`, so this stays
/// testable without risking a real config path.
fn resolve_team_override(
    teams: &[StoredFantasyTeam],
    config: &mut config::Config,
    query: &str,
) -> Result<(String, bool), MatchupError> {
    let resolved = select_matchup_team(teams, query)?.team_key.clone();
    let changed = config.current_team_key != resolved;
    if changed {
        config.current_team_key = resolved.clone();
    }
    Ok((resolved, changed))
}

const YAHOO_SOURCE: &str = "yahoo_public";

/// Render the default weekly matchup view from the public redzone feed.
fn show_weekly_matchup(
    store: &mut Store,
    league_key: &str,
    effective_team_key: Option<String>,
    public_client: &YahooPublicClient,
    public_league_id: &str,
    force_weekly: bool,
    runtime: (Arc<HttpClient>, SystemTime),
) -> Result<String, MatchupError> {
    let (http, now) = runtime;
    let team_key = effective_team_key.ok_or_else(|| {
        MatchupError(
            "match: no primary team selected; run b9 sync -T <key-or-name> and retry".into(),
        )
    })?;
    let mut redzone_feed: Option<Result<RedzoneFeed, String>> = None;
    let current_week = store
        .fantasy_current_week(league_key)
        .map_err(|error| contextual("read current matchup week", error))?;
    let scoreboard_scope = format!(
        "{league_key}:{}",
        current_week.map_or_else(|| "current".into(), |week| week.to_string())
    );
    let (matchups, scoreboard_stale, _) = match cached_or_fetch_any_source_at(
        store,
        "match_scoreboard",
        YAHOO_SOURCE,
        &scoreboard_scope,
        now,
        || -> Result<Vec<Matchup>, String> {
            ensure_redzone_feed(
                public_client,
                &mut redzone_feed,
                public_league_id,
                league_key,
            )
            .map(|feed| feed.matchups.clone())
            .map_err(|error| error.to_string())
        },
    ) {
        Ok(result) => result,
        Err(scoreboard_error) => {
            return match local_matchup_view(store, league_key, &team_key) {
                Ok(view) => Ok(render_local_matchup(&view, detected_help_color_mode())),
                Err(_) => Err(scoreboard_error),
            };
        }
    };
    let matchup = matchups
        .into_iter()
        .find(|matchup| matchup.teams.iter().any(|team| team.team_key == team_key))
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
    let (mut mine, mine_stale, _) = cached_or_fetch_any_source_at(
        store,
        "match_roster",
        YAHOO_SOURCE,
        &mine_scope,
        now,
        || -> Result<RosterWeekStats, String> {
            ensure_redzone_feed(
                public_client,
                &mut redzone_feed,
                public_league_id,
                league_key,
            )
            .map_err(|error| error.to_string())
            .and_then(|feed| {
                feed.roster_week_stats
                    .get(&matchup.teams[my_index].team_key)
                    .cloned()
                    .ok_or_else(|| "no roster available for the selected team".into())
            })
        },
    )?;
    let (mut opponent, opponent_stale, _) = cached_or_fetch_any_source_at(
        store,
        "match_roster",
        YAHOO_SOURCE,
        &opponent_scope,
        now,
        || -> Result<RosterWeekStats, String> {
            ensure_redzone_feed(
                public_client,
                &mut redzone_feed,
                public_league_id,
                league_key,
            )
            .map_err(|error| error.to_string())
            .and_then(|feed| {
                feed.roster_week_stats
                    .get(&matchup.teams[opponent_index].team_key)
                    .cloned()
                    .ok_or_else(|| "no roster available for the opponent team".into())
            })
        },
    )?;
    let daily_date = if !force_weekly {
        Some(utc_date(SystemTime::now())?)
    } else {
        None
    };
    if let Some(day) = &daily_date {
        apply_daily_stats(store, &mut mine, &mut opponent, day, http.clone())?;
    }
    apply_roster_statuses(store, league_key, &mut mine, &mut opponent)?;
    let odds = acquire_odds_context(store, http, &mine, &opponent).unwrap_or_default();
    let stale = scoreboard_stale || mine_stale || opponent_stale;
    let teams = store
        .fantasy_teams(league_key)
        .map_err(|error| contextual("read matchup team context", error))?;
    let view = MatchupView {
        matchup,
        mine,
        opponent,
        teams,
        stale,
        odds,
    };
    let mut output = render_matchup(&view, detected_help_color_mode());
    if let Some(day) = daily_date {
        output = format!("DAY {day}\n{output}");
    }
    Ok(output)
}

/// Fetch the public redzone feed at most once per `show_weekly_matchup`
/// call, regardless of how many of its three cache slots miss — the feed
/// already carries the scoreboard and both rosters together. Caches the
/// failure too, not just success: without that, a failed fetch during the
/// scoreboard step would retry (and fail again) for each roster step that
/// also needs a live fetch.
fn ensure_redzone_feed<'a>(
    client: &YahooPublicClient,
    cache: &'a mut Option<Result<RedzoneFeed, String>>,
    public_league_id: &str,
    league_key: &str,
) -> Result<&'a RedzoneFeed, MatchupError> {
    let result = cache.get_or_insert_with(|| {
        let mut feed = client
            .fetch_redzone(public_league_id, league_key)
            .map_err(|error| contextual("fetch public matchup feed", error).to_string())?;
        feed.matchups = client
            .fetch_scoreboard(league_key, feed.week)
            .map_err(|error| contextual("fetch public matchup totals", error).to_string())?;
        Ok(feed)
    });
    result
        .as_ref()
        .map_err(|message| MatchupError(message.clone()))
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

fn persisted_or_fetch<T, E, F>(
    store: &mut Store,
    dataset: &str,
    scope: &str,
    prefer_persisted: bool,
    fetch: F,
) -> Result<(T, bool), MatchupError>
where
    T: Serialize + DeserializeOwned,
    E: fmt::Display,
    F: FnOnce() -> Result<T, E>,
{
    if prefer_persisted
        && let Some(snapshot) = store
            .command_snapshot(dataset, "yahoo", scope)
            .map_err(|error| contextual("read persisted matchup history", error))?
    {
        let value = serde_json::from_str(&snapshot.payload)
            .map_err(|error| contextual("decode persisted matchup history", error))?;
        return Ok((value, snapshot.stale));
    }
    cached_or_fetch(store, dataset, scope, fetch)
}

fn persisted_roster_or_fetch<E, F>(
    store: &mut Store,
    dataset: &str,
    scope: &str,
    prefer_persisted: bool,
    fetch: F,
) -> Result<(RosterWeekStats, bool), MatchupError>
where
    E: fmt::Display,
    F: FnOnce() -> Result<RosterWeekStats, E>,
{
    if prefer_persisted
        && let Some(snapshot) = store
            .command_snapshot(dataset, "yahoo", scope)
            .map_err(|error| contextual("read persisted roster history", error))?
    {
        let roster = serde_json::from_str::<RosterWeekStats>(&snapshot.payload)
            .map_err(|error| contextual("decode persisted roster history", error))?;
        if snapshot.snapshot_version == "v2"
            && !roster.players.is_empty()
            && roster
                .players
                .iter()
                .all(|player| !player.name.is_empty() && !player.position_type.is_empty())
        {
            return Ok((roster, snapshot.stale));
        }
    }
    let mut roster = fetch().map_err(|error| contextual("refresh historical roster", error))?;
    if roster.players.is_empty() {
        return Err(MatchupError(
            "match: historical roster contains no players; retry or choose another period".into(),
        ));
    }
    enrich_historical_roster(store, &mut roster)?;
    if roster
        .players
        .iter()
        .any(|player| player.name.is_empty() || player.position_type.is_empty())
    {
        return Err(MatchupError(
            "match: historical roster is missing player metadata; run b9 sync and retry".into(),
        ));
    }
    let payload = serde_json::to_string(&roster)
        .map_err(|error| contextual("serialize historical roster", error))?;
    store
        .save_command_snapshot(dataset, "yahoo", scope, "v2", &payload)
        .map_err(|error| contextual("persist historical roster", error))?;
    Ok((roster, false))
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

/// Reuse, refresh, or fall back to a command snapshot, preferring whichever
/// of *any* source's row for this dataset/scope is freshest and not stale
/// (AT4) — used only by `show_weekly_matchup`'s default view, where OAuth
/// and the public feed are equally valid. `write_source` identifies which
/// source a live fetch and its resulting write belong to; the returned
/// `String` is the source the returned value actually came from (which may
/// differ from `write_source` on a cross-source cache hit or fallback).
fn cached_or_fetch_any_source_at<T, E, F>(
    store: &mut Store,
    dataset: &str,
    write_source: &str,
    scope: &str,
    now: SystemTime,
    fetch: F,
) -> Result<(T, bool, String), MatchupError>
where
    T: Serialize + DeserializeOwned,
    E: fmt::Display,
    F: FnOnce() -> Result<T, E>,
{
    let candidates = store
        .command_snapshots_by_scope(dataset, scope)
        .map_err(|error| contextual("read cached data", error))?;
    if let Some(snapshot) = candidates.iter().find(|snapshot| {
        !snapshot.stale
            && now
                .duration_since(snapshot.last_successful_at)
                .unwrap_or_default()
                < MATCHUP_TTL
    }) && let Ok(value) = serde_json::from_str(&snapshot.payload)
    {
        return Ok((value, false, snapshot.source.clone()));
    }
    match fetch() {
        Ok(value) => {
            let payload = serde_json::to_string(&value)
                .map_err(|error| contextual("serialize refreshed data", error))?;
            store
                .save_command_snapshot(dataset, write_source, scope, "v1", &payload)
                .map_err(|error| contextual("save refreshed data", error))?;
            Ok((value, false, write_source.to_owned()))
        }
        Err(error) => {
            let _ =
                store.mark_command_snapshot_stale(dataset, write_source, scope, &error.to_string());
            if let Some(snapshot) = candidates.first() {
                let value = serde_json::from_str(&snapshot.payload)
                    .map_err(|decode| contextual("decode stale data", decode))?;
                Ok((value, true, snapshot.source.clone()))
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
    let mine_team = view
        .teams
        .iter()
        .find(|team| team.team_key == mine.team_key);
    let opponent_team = view
        .teams
        .iter()
        .find(|team| team.team_key == opponent.team_key);
    let mut output = String::new();
    output.push_str(&format!(
        "{} {}\n",
        table_heading("MATCHUP WEEK:", mode),
        dim(
            &format!(
                "{} of 26 ({})",
                view.matchup.week,
                matchup_week_dates(&view.matchup.week_start, &view.matchup.week_end)
            ),
            mode
        )
    ));
    if view.stale {
        output.push_str(&warning(
            "STALE — Yahoo unavailable; showing the last complete matchup snapshot",
            mode,
        ));
        output.push('\n');
    }
    let divider = format!(
        "{}           {}\n",
        matchup_team_divider(mine, mine_team, mode),
        matchup_team_divider(opponent, opponent_team, mode)
    );
    output.push_str(divider.trim_end());
    output.push('\n');
    render_players(
        &mut output,
        &view.mine.players,
        &view.opponent.players,
        "B",
        mine,
        opponent,
        mode,
    );
    output.push('\n');
    render_players(
        &mut output,
        &view.mine.players,
        &view.opponent.players,
        "P",
        mine,
        opponent,
        mode,
    );
    render_matchup_summary(&mut output, mine, view, mode);
    output
}

fn matchup_team_divider(
    matchup: &MatchupTeam,
    team: Option<&StoredFantasyTeam>,
    mode: HelpColorMode,
) -> String {
    let (wins, losses, ties, rank) = team.map_or(
        (
            i64::from(matchup.wins),
            i64::from(matchup.losses),
            i64::from(matchup.ties),
            0,
        ),
        |team| (team.wins, team.losses, team.ties, team.rank),
    );
    let played = matchup.completed_games + matchup.live_games;
    let total = played + matchup.remaining_games;
    let name = available(&clean_fantasy_team_name(&matchup.name), mode);
    let rank = if rank > 0 {
        ordinal(rank)
    } else {
        "—".into()
    };
    let info = dim(
        &format!(
            "({wins}-{losses}-{ties} | {rank}) - {} rem ({played}/{total})",
            matchup.remaining_games
        ),
        mode,
    );
    let mut value = format!("{name} {info}");
    let width = visible_width(&value);
    if width < 67 {
        value.push_str(&" ".repeat(67 - width));
    }
    value
}

fn ordinal(value: i64) -> String {
    let suffix = if (11..=13).contains(&(value % 100)) {
        "th"
    } else {
        match value % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    format!("{value}{suffix}")
}

fn render_matchup_summary(
    output: &mut String,
    mine: &MatchupTeam,
    view: &MatchupView,
    mode: HelpColorMode,
) {
    output.push('\n');
    output.push_str(&table_heading("SUMMARY", mode));
    output.push('\n');
    output.push_str(&format!(
        "{} {} / {} / {}\n",
        table_heading(&format!("{:<12}", "W/T/L"), mode),
        good(&mine.wins.to_string(), mode),
        warning(&mine.ties.to_string(), mode),
        injury_status(&mine.losses.to_string(), mode),
    ));
    render_odds_block(
        output,
        "MY ODDS",
        view.odds.iter().filter(|odds| odds.mine),
        mode,
    );
    render_odds_block(
        output,
        "OPP ODDS",
        view.odds.iter().filter(|odds| !odds.mine),
        mode,
    );
}

fn render_odds_block<'a>(
    output: &mut String,
    label: &str,
    odds: impl Iterator<Item = &'a MatchupOdds>,
    mode: HelpColorMode,
) {
    for (index, odds) in odds.enumerate() {
        let label = if index == 0 {
            table_heading(&format!("{label:<12}"), mode)
        } else {
            " ".repeat(12)
        };
        output.push_str(&format!("{label} {}\n", render_odds_line(&odds.line, mode)));
    }
}

fn matchup_week_dates(start: &str, end: &str) -> String {
    format!("{} / {}", matchup_week_date(start), matchup_week_date(end))
}

fn matchup_week_date(value: &str) -> String {
    let parts = value
        .split('-')
        .filter_map(|part| part.parse::<i64>().ok())
        .collect::<Vec<_>>();
    let [year, month, day] = parts.as_slice() else {
        return value.to_owned();
    };
    let month_name = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ]
    .get((*month - 1) as usize)
    .copied()
    .unwrap_or("");
    if month_name.is_empty() {
        return value.to_owned();
    }
    let adjusted_year = year - i64::from(*month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = month + if *month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    let weekday =
        ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][(days + 4).rem_euclid(7) as usize];
    format!("{weekday} {month_name}-{day:02}")
}

/// Render the deliberately limited fallback without matchup claims.
pub fn render_local_matchup(view: &LocalMatchupView, mode: HelpColorMode) -> String {
    let mut output = format!(
        "{}\n",
        warning(
            "YAHOO UNAVAILABLE — showing local roster; matchup totals and opponent unavailable",
            mode
        )
    );
    output.push_str(&render_roster_players(
        &clean_fantasy_team_name(&view.team_name),
        &view.players,
        mode,
    ));
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
        .collect();
    Ok(LocalMatchupView {
        team_name: team.name,
        players,
    })
}

fn acquire_odds_context(
    store: &mut Store,
    http: Arc<HttpClient>,
    mine: &RosterWeekStats,
    opponent: &RosterWeekStats,
) -> Result<Vec<MatchupOdds>, MatchupError> {
    let now = SystemTime::now();
    let date = utc_date(now)?;
    let schedule = crate::providers::mlb::MlbClient::production(http.clone())
        .fetch_schedule(&date)
        .map_err(|error| contextual("refresh MLB schedule", error))?;
    let cached = store
        .command_snapshot("mlb_current_odds", "espn", &date)
        .map_err(|error| contextual("read synchronized ESPN odds", error))?;
    let lines = if let Some(snapshot) = cached.as_ref().filter(|snapshot| {
        !snapshot.stale
            && now
                .duration_since(snapshot.last_successful_at)
                .unwrap_or(Duration::MAX)
                <= Duration::from_secs(30 * 60)
    }) {
        serde_json::from_str(&snapshot.payload)
            .map_err(|error| contextual("decode synchronized ESPN odds", error))?
    } else {
        match crate::providers::espn::EspnClient::production(http).fetch_game_lines(now) {
            Ok(lines) => {
                let payload = serde_json::to_string(&lines)
                    .map_err(|error| contextual("encode refreshed ESPN odds", error))?;
                store
                    .save_command_snapshot("mlb_current_odds", "espn", &date, "1", &payload)
                    .map_err(|error| contextual("cache refreshed ESPN odds", error))?;
                lines
            }
            Err(error) => {
                if let Some(snapshot) = cached {
                    serde_json::from_str(&snapshot.payload)
                        .map_err(|decode| contextual("decode stale ESPN odds", decode))?
                } else {
                    return Err(contextual("refresh ESPN odds", error));
                }
            }
        }
    };
    let mut output = Vec::new();
    for game in schedule {
        if let Some(line) = lines.games.iter().find(|line| {
            normalized_team(&line.home_team) == normalized_team(&game.home_team_name)
                && normalized_team(&line.away_team) == normalized_team(&game.away_team_name)
        }) && line.quoted
        {
            output.extend(rostered_probable_odds(&game, line, mine, opponent));
        }
    }
    Ok(output)
}

fn roster_has_probable_pitcher(
    game: &ScheduleGame,
    mine: &RosterWeekStats,
    opponent: &RosterWeekStats,
) -> bool {
    let probable = [
        game.away_probable_pitcher_name.as_str(),
        game.home_probable_pitcher_name.as_str(),
    ]
    .map(normalized_team);
    mine.players
        .iter()
        .chain(&opponent.players)
        .filter(|player| player.position_type == "P")
        .any(|player| probable.contains(&normalized_team(&player.name)))
}

fn rostered_probable_odds(
    game: &ScheduleGame,
    line: &crate::providers::espn::GameLine,
    mine: &RosterWeekStats,
    opponent: &RosterWeekStats,
) -> Vec<MatchupOdds> {
    if !roster_has_probable_pitcher(game, mine, opponent) {
        return Vec::new();
    }
    let rostered_pitchers = mine
        .players
        .iter()
        .chain(&opponent.players)
        .filter(|player| player.position_type == "P")
        .collect::<Vec<_>>();
    let (away_probability, home_probability) =
        normalized_probabilities(line.away_moneyline, line.home_moneyline);
    let away_team = crate::player_commands::mlb_team_abbreviation(game.away_team_id);
    let home_team = crate::player_commands::mlb_team_abbreviation(game.home_team_id);
    let tag = format!("{away_team}@{home_team}");
    [
        (
            game.away_probable_pitcher_name.as_str(),
            game.home_probable_pitcher_name.as_str(),
            away_probability,
        ),
        (
            game.home_probable_pitcher_name.as_str(),
            game.away_probable_pitcher_name.as_str(),
            home_probability,
        ),
    ]
    .into_iter()
    .filter_map(|(pitcher, opposing_pitcher, probability)| {
        let player = rostered_pitchers
            .iter()
            .find(|player| normalized_team(&player.name) == normalized_team(pitcher))?;
        let mine = mine
            .players
            .iter()
            .any(|mine| mine.yahoo_player_id == player.yahoo_player_id);
        Some((pitcher, opposing_pitcher, probability, mine))
    })
    .map(|(pitcher, opposing_pitcher, probability, mine)| {
        let percent = (probability * 100.0).round() as usize;
        let filled = ((percent + 5) / 10).min(10);
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled));
        MatchupOdds {
            mine,
            line: format!(
                "{:<16} v {:<16}  {:<7}  {bar} {percent}%",
                last_name(pitcher),
                last_name(opposing_pitcher),
                tag,
            ),
        }
    })
    .collect()
}

fn last_name(name: &str) -> &str {
    let name = name.split_once(" (").map_or(name, |(name, _)| name);
    name.split_whitespace().last().unwrap_or("")
}

fn render_odds_line(line: &str, mode: HelpColorMode) -> String {
    if mode == HelpColorMode::Plain {
        return line.to_owned();
    }
    let Some(bar_start) = line.find(['█', '░']) else {
        return line.to_owned();
    };
    let percent = line[bar_start..]
        .split_whitespace()
        .last()
        .and_then(|value| value.strip_suffix('%'))
        .and_then(|value| value.parse::<usize>().ok());
    let Some(percent) = percent else {
        return line.to_owned();
    };
    let code = if percent >= 50 { "38;5;34" } else { "38;5;196" };
    format!(
        "{}\u{1b}[{code}m{}\u{1b}[0m",
        &line[..bar_start],
        &line[bar_start..]
    )
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

fn render_players(
    output: &mut String,
    mine: &[PlayerWeekStats],
    opponent: &[PlayerWeekStats],
    role: &str,
    mine_team: &MatchupTeam,
    opponent_team: &MatchupTeam,
    mode: HelpColorMode,
) {
    const NAME_WIDTH: usize = 20;
    const CELL_WIDTH: usize = 67;
    let left = mine
        .iter()
        .filter(|player| player.position_type == role)
        .collect::<Vec<_>>();
    let right = opponent
        .iter()
        .filter(|player| player.position_type == role)
        .collect::<Vec<_>>();
    let header = if role == "B" {
        format!(
            "{}{}{}{}{}{}{}{}",
            table_heading(&format!("{:<NAME_WIDTH$}", "HITTER"), mode),
            table_heading(&format!("{:<17}", "STATUS"), mode),
            dim(&format!("{:>6}", "H/AB"), mode),
            table_heading(&format!("{:>4}", "R"), mode),
            table_heading(&format!("{:>4}", "HR"), mode),
            table_heading(&format!("{:>4}", "RBI"), mode),
            table_heading(&format!("{:>5}", "SB"), mode),
            table_heading(&format!("{:>7}", "AVG"), mode),
        )
    } else {
        format!(
            "{}{}{}{}{}{}{}{}",
            table_heading(&format!("{:<NAME_WIDTH$}", "PITCHER"), mode),
            table_heading(&format!("{:<17}", "STATUS"), mode),
            dim(&format!("{:>6}", "IP"), mode),
            table_heading(&format!("{:>4}", "W"), mode),
            table_heading(&format!("{:>4}", "SV"), mode),
            table_heading(&format!("{:>4}", "K"), mode),
            table_heading(&format!("{:>6}", "ERA"), mode),
            table_heading(&format!("{:>6}", "WHIP"), mode),
        )
    };
    output.push_str(&format!(
        "{header}    {}   {header}\n",
        dim(&format!("{:<4}", "SLOT"), mode)
    ));
    let rows = left.len().max(right.len());
    for index in 0..rows {
        let left_player = left.get(index).copied();
        let right_player = right.get(index).copied();
        let left = left_player
            .map(|player| matchup_player_cell(player, role, mode))
            .unwrap_or_else(|| " ".repeat(CELL_WIDTH));
        let right = right_player
            .map(|player| matchup_player_cell(player, role, mode))
            .unwrap_or_default();
        let slot = left_player
            .map(|player| player.slot_position.to_string())
            .unwrap_or_default();
        let row = format!("{left}    {}   {right}\n", dim(&format!("{slot:<4}"), mode));
        output.push_str(row.trim_end());
        output.push('\n');
    }
    render_matchup_totals(output, role, mine_team, opponent_team, mode);
}

fn render_matchup_totals(
    output: &mut String,
    role: &str,
    mine: &MatchupTeam,
    opponent: &MatchupTeam,
    mode: HelpColorMode,
) {
    const NAME_AND_STATUS_WIDTH: usize = 37;
    let categories = if role == "B" {
        [
            ("H/AB", "60", 6, false),
            ("R", "7", 4, false),
            ("HR", "12", 4, false),
            ("RBI", "13", 4, false),
            ("SB", "16", 5, false),
            ("AVG", "3", 7, false),
        ]
    } else {
        [
            ("IP", "50", 6, false),
            ("W", "28", 4, false),
            ("SV", "32", 4, false),
            ("K", "42", 4, false),
            ("ERA", "26", 6, true),
            ("WHIP", "27", 6, true),
        ]
    };
    let side = |team: &MatchupTeam, mine_side: bool| {
        let mut rendered = " ".repeat(NAME_AND_STATUS_WIDTH);
        for (index, (name, id, width, lower_wins)) in categories.iter().enumerate() {
            let value = matchup_stat(team, name, id);
            let padded = format!("{value:>width$}");
            if index == 0 {
                rendered.push_str(&dim(&padded, mode));
            } else {
                let other = if mine_side { opponent } else { mine };
                rendered.push_str(&matchup_total_color(
                    &padded,
                    value,
                    matchup_stat(other, name, id),
                    *lower_wins,
                    mode,
                ));
            }
        }
        rendered
    };
    output.push_str(&format!(
        "{}           {}\n",
        side(mine, true),
        side(opponent, false)
    ));
}

fn matchup_stat<'a>(team: &'a MatchupTeam, name: &str, id: &str) -> &'a str {
    team.stats
        .get(name)
        .or_else(|| team.stats.get(id))
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("—")
}

fn matchup_total_color(
    padded: &str,
    value: &str,
    opponent: &str,
    lower_wins: bool,
    mode: HelpColorMode,
) -> String {
    if mode == HelpColorMode::Plain {
        return padded.to_owned();
    }
    let code = match (value.parse::<f64>(), opponent.parse::<f64>()) {
        (Ok(value), Ok(opponent)) if value != opponent => {
            if (value > opponent) != lower_wins {
                "1;38;5;34"
            } else {
                "1;38;5;196"
            }
        }
        _ => "1;38;5;231",
    };
    format!("\u{1b}[{code}m{padded}\u{1b}[0m")
}

fn matchup_player_cell(player: &PlayerWeekStats, role: &str, mode: HelpColorMode) -> String {
    let name = matchup_player_name(player);
    let status = if player.injury_status.is_empty() {
        "NoGame"
    } else {
        &player.injury_status
    };
    let stats = if role == "B" {
        format!(
            "{:>6}{:>4}{:>4}{:>4}{:>5}{:>7}",
            if player.hab.is_empty() {
                "—"
            } else {
                &player.hab
            },
            player.runs,
            player.home_runs,
            player.runs_batted_in,
            player.stolen_bases,
            if player.batting_average.is_empty() {
                "—"
            } else {
                &player.batting_average
            },
        )
    } else {
        format!(
            "{:>6}{:>4}{:>4}{:>4}{:>6}{:>6}",
            if player.innings_pitched.is_empty() {
                "—"
            } else {
                &player.innings_pitched
            },
            player.wins,
            player.saves,
            player.strikeouts,
            if player.earned_run_average.is_empty() {
                "—"
            } else {
                &player.earned_run_average
            },
            if player.whip.is_empty() {
                "—"
            } else {
                &player.whip
            },
        )
    };
    let status = format!("{:<17}", status.chars().take(17).collect::<String>());
    let status = style_matchup_status(&status, role, player.slot_position == Position::Bench, mode);
    let row = format!("{name}{status}{stats}");
    if player.slot_position == Position::InjuredList || player.injury_status.starts_with("IL") {
        warning(&row, mode)
    } else if player.slot_position == Position::Bench {
        dim(&row, mode)
    } else {
        row
    }
}

fn style_matchup_status(status: &str, role: &str, subdued: bool, mode: HelpColorMode) -> String {
    let Some(marker) = status.split_whitespace().find(|value| {
        *value == "●" || matches!(*value, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
    }) else {
        return status.to_owned();
    };
    let favorable = marker != "●" || role == "P";
    let needle = format!(" {marker} ");
    let replacement = format!(" {} ", lineup_indicator(marker, favorable, subdued, mode));
    status.replacen(&needle, &replacement, 1)
}

fn matchup_player_name(player: &PlayerWeekStats) -> String {
    const NAME_WIDTH: usize = 20;
    let short_name = player.name.split_once(' ').map_or_else(
        || player.name.clone(),
        |(first, rest)| {
            first
                .chars()
                .next()
                .map_or_else(|| rest.to_owned(), |initial| format!("{initial} {rest}"))
        },
    );
    let team_width = player.team.chars().count();
    let value = if player.team.is_empty() {
        short_name.chars().take(NAME_WIDTH).collect::<String>()
    } else {
        let max_name_width = NAME_WIDTH.saturating_sub(team_width + 3);
        format!(
            "{} {}",
            short_name.chars().take(max_name_width).collect::<String>(),
            player.team
        )
    };
    format!("{value:<NAME_WIDTH$}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_day_uses_active_season_and_rejects_long_form() {
        assert_eq!(season_day("jUl-01", 2026).unwrap(), "2026-07-01");
        assert!(season_day("feb-29", 2026).is_err());
        assert!(short_day_parts("2026-07-01").is_none());
    }
    use crate::providers::mlb::{BulkHittingSplit, BulkPitchingSplit, HittingStats, PitchingStats};
    use crate::transport::{ExecutorError, HttpExecutor, HttpResponse, ValidatedRequest};

    #[test]
    fn independent_daily_stat_fetches_run_on_workers() {
        let caller = thread::current().id();
        let (left, right) = parallel_pair(|| thread::current().id(), || thread::current().id());

        assert_ne!(left.unwrap(), caller);
        assert_ne!(right.unwrap(), caller);
    }

    #[test]
    fn odds_include_only_games_with_a_rostered_probable_pitcher() {
        let game = ScheduleGame {
            game_id: 1,
            game_date: "2026-08-18T22:40:00Z".into(),
            detailed_state: "Scheduled".into(),
            away_team_id: 116,
            away_team_name: "Detroit Tigers".into(),
            home_team_id: 134,
            home_team_name: "Pittsburgh Pirates".into(),
            away_probable_pitcher_id: Some(10),
            away_probable_pitcher_name: "Ada Starter".into(),
            home_probable_pitcher_id: Some(20),
            home_probable_pitcher_name: "Grace Starter".into(),
            linescore: None,
            away_lineup: None,
            home_lineup: None,
        };
        let roster = |name: &str, role: &str| RosterWeekStats {
            team_key: "team".into(),
            team_name: "Team".into(),
            week: 1,
            players: vec![PlayerWeekStats {
                yahoo_player_id: 1,
                name: name.into(),
                team: "NYY".into(),
                position_type: role.into(),
                slot_position: Position::StartingPitcher,
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
            }],
        };
        let empty = RosterWeekStats {
            team_key: "other".into(),
            team_name: "Other".into(),
            week: 1,
            players: vec![],
        };

        assert!(roster_has_probable_pitcher(
            &game,
            &roster("Ada Starter", "P"),
            &empty
        ));
        assert!(!roster_has_probable_pitcher(
            &game,
            &roster("Ada Starter", "B"),
            &empty
        ));
        assert!(!roster_has_probable_pitcher(
            &game,
            &roster("Other Pitcher", "P"),
            &empty
        ));
        let rows = rostered_probable_odds(
            &game,
            &crate::providers::espn::GameLine {
                event_id: "1".into(),
                competition_id: "1".into(),
                home_team: "Pittsburgh Pirates".into(),
                away_team: "Detroit Tigers".into(),
                sportsbook: "test".into(),
                home_moneyline: -133,
                away_moneyline: 133,
                quoted: true,
            },
            &roster("Ada Starter", "P"),
            &empty,
        );
        assert_eq!(rows.len(), 1);
        assert!(rows[0].mine);
        assert!(rows[0].line.starts_with("Starter          v Starter"));
        assert!(rows[0].line.ends_with("  DET@PIT  ████░░░░░░ 43%"));
        assert!(
            render_odds_line(&rows[0].line, HelpColorMode::Color)
                .contains("\u{1b}[38;5;196m████░░░░░░ 43%\u{1b}[0m")
        );
    }

    #[test]
    fn matchup_statuses_reuse_roster_injury_and_game_values() {
        let mut players = vec![PlayerWeekStats {
            yahoo_player_id: 1,
            name: "Ada Starter".into(),
            team: "NYY".into(),
            position_type: "P".into(),
            slot_position: Position::StartingPitcher,
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
        let stored = |status: &str, game_status: &str| StoredFantasyPlayer {
            yahoo_player_id: Some(1),
            mlbam_id: Some(10),
            name: "Ada Starter".into(),
            team: "NYY".into(),
            role: "P".into(),
            positions: "SP".into(),
            is_closer: false,
            status: status.into(),
            injury_note: String::new(),
            birth_date: String::new(),
            game_status: game_status.into(),
            game_indicator: crate::domain::GameIndicator::StartingPitcher,
            hand: "R".into(),
            rank: None,
            percent_owned: None,
            percentage_started: 0.0,
            expert_consensus_rank: None,
            owner: None,
            slot: Some("SP".into()),
            batting: [0.0; 7],
            pitching: [0.0; 7],
            hitting_advanced: [None; 8],
            pitching_advanced: [None; 6],
            fangraphs_batted_ball: [None; 2],
            pqs_counting: [0.0; 6],
            statcast_samples: [0.0; 4],
            pqs_prior_counting: [0.0; 6],
            league_games_played: 0,
        };

        apply_resolved_roster_statuses(&mut players, &[stored("", "7:05p ● v BOS")]);
        assert_eq!(players[0].injury_status, "7:05p ● v BOS");
        apply_resolved_roster_statuses(&mut players, &[stored("IL15", "7:05p ● v BOS")]);
        assert_eq!(players[0].injury_status, "IL15");
    }
    use std::collections::VecDeque;
    use std::sync::Mutex;

    const REDZONE_VALID: &[u8] =
        include_bytes!("../tests/fixtures/yahoo-public/redzone_valid.json");

    struct FixedExecutor {
        responses: Mutex<VecDeque<Result<HttpResponse, ExecutorError>>>,
    }

    impl FixedExecutor {
        fn new(responses: Vec<Result<HttpResponse, ExecutorError>>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
            }
        }
    }

    impl HttpExecutor for FixedExecutor {
        fn execute(&self, _request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .expect("fixed response available")
        }
    }

    fn public_client(responses: Vec<Result<HttpResponse, ExecutorError>>) -> YahooPublicClient {
        YahooPublicClient::new(HttpClient::new(Arc::new(FixedExecutor::new(responses))))
    }

    fn ok_response() -> Result<HttpResponse, ExecutorError> {
        Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: REDZONE_VALID.to_vec(),
        })
    }

    fn scoreboard_response() -> Result<HttpResponse, ExecutorError> {
        Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: br#"{"data":[{"week":20,"week_start":"2026-08-10","week_end":"2026-08-16","status":"midevent","teams":[{"team_key":"469.l.170874.t.1","team_id":1,"name":"New York Yankees","team_stats":{"stats":[{"stat_id":60,"value":"12/40"},{"stat_id":7,"value":"9"},{"stat_id":12,"value":"3"},{"stat_id":13,"value":"8"},{"stat_id":16,"value":"2"},{"stat_id":3,"value":".300"},{"stat_id":50,"value":"6.0"},{"stat_id":28,"value":"1"},{"stat_id":32,"value":"0"},{"stat_id":42,"value":"10"},{"stat_id":26,"value":"3.00"},{"stat_id":27,"value":"1.33"}]}},{"team_key":"469.l.170874.t.2","team_id":2,"name":"Stuntin' Like My Vladdy","team_stats":{"stats":[{"stat_id":60,"value":"10/40"},{"stat_id":7,"value":"7"},{"stat_id":12,"value":"2"},{"stat_id":13,"value":"6"},{"stat_id":16,"value":"1"},{"stat_id":3,"value":".250"},{"stat_id":50,"value":"7.0"},{"stat_id":28,"value":"0"},{"stat_id":32,"value":"1"},{"stat_id":42,"value":"8"},{"stat_id":26,"value":"4.50"},{"stat_id":27,"value":"1.50"}]}}]}]}"#
                .to_vec(),
        })
    }

    fn blocked_response() -> Result<HttpResponse, ExecutorError> {
        Ok(HttpResponse {
            status: 403,
            headers: Vec::new(),
            body: Vec::new(),
        })
    }

    /// A fake `http` boundary for `show_weekly_matchup`'s unconditional odds
    /// lookup — it swallows failures (`.unwrap_or_default()`), so one queued
    /// error response is enough to make it a harmless, hermetic no-op.
    fn failing_http_client() -> Arc<HttpClient> {
        Arc::new(HttpClient::new(Arc::new(FixedExecutor::new(vec![
            blocked_response(),
        ]))))
    }

    fn matchup_team(team_key: &str, name: &str) -> MatchupTeam {
        MatchupTeam {
            team_key: team_key.into(),
            team_id: 1,
            name: name.into(),
            is_current_login: false,
            stats: HashMap::new(),
            wins: 0,
            losses: 0,
            ties: 0,
            completed_games: 0,
            live_games: 0,
            remaining_games: 0,
        }
    }

    fn matchup_fixture(team_a: &str, team_b: &str, week: i32) -> Matchup {
        Matchup {
            week,
            week_start: "2026-08-10".into(),
            week_end: "2026-08-16".into(),
            status: String::new(),
            teams: [
                matchup_team(team_a, "Team A"),
                matchup_team(team_b, "Team B"),
            ],
        }
    }

    fn empty_roster(team_key: &str, week: i32) -> RosterWeekStats {
        RosterWeekStats {
            team_key: team_key.into(),
            team_name: String::new(),
            week,
            players: Vec::new(),
        }
    }

    fn stored_team(team_key: &str, name: &str, manager: &str) -> StoredFantasyTeam {
        StoredFantasyTeam {
            team_key: team_key.into(),
            name: name.into(),
            manager_name: manager.into(),
            team_id: 1,
            waiver_priority: 0,
            faab_balance: 0,
            wins: 0,
            losses: 0,
            ties: 0,
            moves: 0,
            rank: 0,
        }
    }

    #[test]
    fn select_matchup_team_matches_by_name_or_manager_and_reports_ambiguity() {
        let teams = vec![
            stored_team("l.1.t.1", "New York Yankees", "Alice"),
            stored_team("l.1.t.2", "Stuntin' Like My Vladdy", "Bob"),
            stored_team("l.1.t.3", "New Jersey Yetis", "Carol"),
        ];

        assert_eq!(
            select_matchup_team(&teams, "vladdy").unwrap().team_key,
            "l.1.t.2"
        );
        assert_eq!(
            select_matchup_team(&teams, "bob").unwrap().team_key,
            "l.1.t.2"
        );
        assert!(select_matchup_team(&teams, "nobody").is_err());
        let ambiguous = select_matchup_team(&teams, "new").unwrap_err().to_string();
        assert!(ambiguous.contains("ambiguous"));
    }

    #[test]
    fn resolve_team_override_persists_only_on_change_and_never_touches_the_real_config_path() {
        let teams = vec![
            stored_team("l.1.t.1", "New York Yankees", "Alice"),
            stored_team("l.1.t.2", "Stuntin' Like My Vladdy", "Bob"),
        ];
        let mut config = config::Config::default();

        let (resolved, changed) = resolve_team_override(&teams, &mut config, "vladdy").unwrap();
        assert_eq!(resolved, "l.1.t.2");
        assert!(changed);
        assert_eq!(config.current_team_key, "l.1.t.2");

        // Re-resolving to the same team reports no change, matching sync's
        // persist-once-then-reuse pattern for the caller's write gate.
        let (resolved_again, changed_again) =
            resolve_team_override(&teams, &mut config, "vladdy").unwrap();
        assert_eq!(resolved_again, "l.1.t.2");
        assert!(!changed_again);

        assert!(resolve_team_override(&teams, &mut config, "nobody").is_err());
    }

    #[test]
    fn weekly_matchup_uses_public_feed_and_reuses_its_cache() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let public = public_client(vec![ok_response(), scoreboard_response()]);
        let now = SystemTime::now();

        let first = show_weekly_matchup(
            &mut store,
            "469.l.170874",
            Some("469.l.170874.t.1".into()),
            &public,
            "170874",
            true,
            (failing_http_client(), now),
        )
        .unwrap();
        assert!(first.contains("New York Yankees"));
        assert!(first.contains("Stuntin' Like My Vladdy"));
        assert!(!first.contains("STALE"));
        // Public-feed data is never MLBAM-reconciled, so the implicit daily
        // overlay must not silently no-op under a misleading "DAY" label.
        assert!(!first.contains("DAY "));

        // A second call shortly after, with a client that has no queued
        // response left, only succeeds if the cache is actually reused.
        let empty_public = public_client(vec![]);
        let second = show_weekly_matchup(
            &mut store,
            "469.l.170874",
            Some("469.l.170874.t.1".into()),
            &empty_public,
            "170874",
            true,
            (failing_http_client(), now + Duration::from_secs(1)),
        )
        .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn weekly_matchup_falls_back_to_the_stale_snapshot_when_a_later_fetch_fails() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let now = SystemTime::now();

        show_weekly_matchup(
            &mut store,
            "469.l.170874",
            Some("469.l.170874.t.1".into()),
            &public_client(vec![ok_response(), scoreboard_response()]),
            "170874",
            true,
            (failing_http_client(), now),
        )
        .unwrap();

        // Past the TTL, so the failing fetch is actually attempted instead
        // of short-circuiting on the still-fresh cache.
        let later = now + MATCHUP_TTL + Duration::from_secs(1);
        let fallback = show_weekly_matchup(
            &mut store,
            "469.l.170874",
            Some("469.l.170874.t.1".into()),
            &public_client(vec![blocked_response()]),
            "170874",
            true,
            (failing_http_client(), later),
        )
        .unwrap();
        assert!(fallback.contains("STALE"));
        assert!(fallback.contains("New York Yankees"));
    }

    #[test]
    fn weekly_matchup_with_no_prior_snapshot_reports_the_fetch_error() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let error = show_weekly_matchup(
            &mut store,
            "469.l.170874",
            Some("469.l.170874.t.1".into()),
            &public_client(vec![blocked_response()]),
            "170874",
            true,
            (failing_http_client(), SystemTime::now()),
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("403"));
        assert!(message.contains("retry later"));
        assert!(!message.contains("login"));
    }

    #[test]
    fn weekly_matchup_reads_a_fresh_historical_public_pull_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let league_key = "469.l.170874";
        let team_key = "469.l.170874.t.1";
        let opponent_key = "469.l.170874.t.2";

        // Seed a `public_pull`-sourced cache for scoreboard and both
        // rosters, all fresh and non-stale.
        let matchup = matchup_fixture(team_key, opponent_key, 20);
        store
            .save_command_snapshot(
                "match_scoreboard",
                "public_pull",
                "469.l.170874:current",
                "v1",
                &serde_json::to_string(&vec![matchup]).unwrap(),
            )
            .unwrap();
        store
            .save_command_snapshot(
                "match_roster",
                "public_pull",
                &format!("{team_key}:20"),
                "v1",
                &serde_json::to_string(&empty_roster(team_key, 20)).unwrap(),
            )
            .unwrap();
        store
            .save_command_snapshot(
                "match_roster",
                "public_pull",
                &format!("{opponent_key}:20"),
                "v1",
                &serde_json::to_string(&empty_roster(opponent_key, 20)).unwrap(),
            )
            .unwrap();

        let output = show_weekly_matchup(
            &mut store,
            league_key,
            Some(team_key.into()),
            &public_client(vec![]),
            "170874",
            true,
            (failing_http_client(), SystemTime::now()),
        )
        .unwrap();
        assert!(output.contains("Team A"));
        assert!(output.contains("Team B"));
        assert!(!output.contains("DAY "));
    }

    #[test]
    fn daily_projection_replaces_matched_stats_and_zeros_absent_splits() {
        let mut players = vec![
            PlayerWeekStats {
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
            },
            PlayerWeekStats {
                yahoo_player_id: 8,
                name: "Grace".into(),
                team: "B9".into(),
                position_type: "P".into(),
                slot_position: Position::StartingPitcher,
                eligible_positions: vec![],
                injury_status: String::new(),
                hab: "3-11".into(),
                runs: 2,
                home_runs: 1,
                runs_batted_in: 4,
                stolen_bases: 1,
                batting_average: "0.273".into(),
                innings_pitched: "6.0".into(),
                wins: 1,
                saves: 1,
                strikeouts: 8,
                earned_run_average: "1.50".into(),
                whip: "0.83".into(),
            },
        ];
        let hitting = vec![BulkHittingSplit {
            player: crate::providers::mlb::BulkPlayer {
                person_id: 42,
                full_name: String::new(),
            },
            team: Default::default(),
            position: Default::default(),
            stat: HittingStats {
                at_bats: 4,
                hits: 2,
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
                players[0].strikeouts,
                players[1].hab.as_str(),
                players[1].innings_pitched.as_str(),
                players[1].earned_run_average.as_str(),
            ),
            ("2-4", 1, 8, "0-0", "0.0", "0.00")
        );
    }

    #[test]
    fn daily_identity_gate_fails_before_provider_acquisition() {
        let mine = RosterWeekStats {
            team_key: "l.1.t.1".into(),
            team_name: "Mine".into(),
            week: 1,
            players: vec![PlayerWeekStats {
                yahoo_player_id: 7,
                name: "Missing Hitter".into(),
                team: "NYY".into(),
                position_type: "B".into(),
                slot_position: Position::Outfield,
                eligible_positions: vec![Position::Outfield],
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
            }],
        };
        let opponent = RosterWeekStats {
            team_key: "l.1.t.2".into(),
            team_name: "Opponent".into(),
            week: 1,
            players: Vec::new(),
        };
        let error = required_mlb_identities(Vec::new(), &mine, &opponent)
            .unwrap_err()
            .to_string();
        assert!(error.contains("run b9 sync and retry"));
        assert!(error.contains("Missing Hitter"));
    }
}
