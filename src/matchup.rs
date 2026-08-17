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
    clean_fantasy_team_name, is_valid_iso_date,
};
use crate::providers::advisory::{AdvisoryClient, AdvisoryProvider};
use crate::providers::mlb::{BulkHittingSplit, BulkPitchingSplit, MlbClient};
use crate::providers::yahoo::YahooClient;
use crate::providers::yahoo_fantasy::{YahooFantasyClient, YahooFantasySource};
use crate::providers::yahoo_public::{RedzoneFeed, YahooPublicClient, league_id_from_key};
use crate::store::{Store, StoredFantasyTeam};
use crate::terminal::{
    HelpColorMode, detected_help_color_mode, dim, table_heading, title, warning,
};
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
    show_with_team_options(league_override, None, options)
}

/// Acquire and render a matchup, with an optional explicit team selector.
///
/// The team argument resolves "my team" by name/manager substring, matching
/// `r <team>`'s existing lookup. When it resolves, it's persisted to
/// `config.current_team_key` (mirroring `pp -l`'s persist-once pattern) so
/// it's needed only once, not on every invocation — unlike the pre-existing
/// `league_override` path below, which re-resolves "my team" live via OAuth
/// on every call and never saves it.
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
    let yahoo = Arc::new(
        YahooClient::production(http.clone())
            .map_err(|error| contextual("initialize Yahoo", error))?,
    );
    let oauth_available = yahoo
        .token_status()
        .is_ok_and(|status| status.valid || status.has_refresh);
    let effective_team_key = resolved_team_override
        .clone()
        .or_else(|| (!config.current_team_key.is_empty()).then(|| config.current_team_key.clone()));

    // The default weekly view (no explicit day/week/advise) is the only one
    // eligible to prefer the public feed — day-mode and advisory-mode stay
    // strictly OAuth-sourced (per settled Out-of-Scope: pp-fetched players
    // aren't MLBAM-identity-reconciled, so a daily overlay over them would
    // silently no-op). Reads there always prefer the freshest non-stale row
    // across *both* sources (AT4); day/advise/explicit-week below are
    // completely untouched and keep reading only the "yahoo" source.
    if options.day.is_none() && !options.advise && options.week.is_none() {
        let public_league_id = league_id_from_key(&league_key)
            .or_else(|_| league_id_from_key(&config.pull_public_league_id))
            .ok();
        let source = YahooFantasyClient::new(yahoo);
        let public_client = YahooPublicClient::production()
            .map_err(|error| contextual("initialize public feed client", error))?;
        return show_weekly_matchup(
            &mut store,
            &league_key,
            effective_team_key,
            oauth_available,
            &source,
            &public_client,
            public_league_id.as_deref(),
            options.weekly,
            http,
            SystemTime::now(),
        );
    }

    if effective_team_key.is_none() && league_override.is_none() {
        return Err(MatchupError(
            "match: no authenticated team stored; run b9 sync and retry".into(),
        ));
    }
    let source = YahooFantasyClient::new(yahoo);
    let current_week = store
        .fantasy_current_week(&league_key)
        .map_err(|error| contextual("read current matchup week", error))?;
    let week = match (&options.day, options.week, options.weekly) {
        (_, Some(week), _) => Some(week),
        (_, None, true) | (None, None, false) => current_week,
        (Some(day), None, false) => Some(resolve_day_week(
            &mut store,
            &source,
            &league_key,
            current_week,
            day,
        )?),
    };
    let scoreboard_scope = format!(
        "{}:{}",
        league_key,
        week.map_or_else(|| "current".into(), |week| week.to_string())
    );
    let team_key = match effective_team_key {
        Some(team_key) => team_key,
        None => source
            .team_key(&league_key)
            .map_err(|error| contextual("resolve authenticated team", error))?,
    };
    let (matchups, scoreboard_stale) =
        match cached_or_fetch(&mut store, "match_scoreboard", &scoreboard_scope, || {
            source.scoreboard(&league_key, week)
        }) {
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
            &league_key,
            &mut mine,
            &mut opponent,
            day,
            http.clone(),
        )?;
    }
    let odds = acquire_odds_context(&mut store, http.clone()).unwrap_or_default();
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
        let response = acquire_advisory(&store, &league_key, &config, &view, http)?;
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
/// mutation in memory, mirroring `public_pull::resolve_league`'s split: only
/// the caller touches `config::write`, so this stays testable without ever
/// risking a real config path.
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

const YAHOO_SOURCE: &str = "yahoo";
const PUBLIC_PULL_SOURCE: &str = "public_pull";

/// Render the default weekly matchup view, preferring whichever of OAuth or
/// the public feed is fresher and non-stale for each cached slot (AT4) —
/// day-mode and advisory-mode never call this; they stay on `cached_or_fetch`
/// and its single "yahoo" source. `public_league_id` is `None` when it can't
/// be resolved at all (no prior `pp`/OAuth login); that only becomes fatal
/// if OAuth also isn't available.
#[allow(clippy::too_many_arguments)]
fn show_weekly_matchup(
    store: &mut Store,
    league_key: &str,
    effective_team_key: Option<String>,
    oauth_available: bool,
    source: &impl YahooFantasySource,
    public_client: &YahooPublicClient,
    public_league_id: Option<&str>,
    force_weekly: bool,
    http: Arc<HttpClient>,
    now: SystemTime,
) -> Result<String, MatchupError> {
    let active_source = if oauth_available {
        YAHOO_SOURCE
    } else {
        PUBLIC_PULL_SOURCE
    };
    let team_key = match effective_team_key {
        Some(team_key) => team_key,
        None if oauth_available => source
            .team_key(league_key)
            .map_err(|error| contextual("resolve authenticated team", error))?,
        None => {
            return Err(MatchupError(
                "match: no team known for the public feed; run b9 m <team> once and retry".into(),
            ));
        }
    };
    let public_league_id = match (oauth_available, public_league_id) {
        (false, Some(id)) => Some(id),
        (false, None) => {
            return Err(MatchupError(
                "match: no public league id resolved; run b9 pp -l <id> once and retry".into(),
            ));
        }
        (true, id) => id,
    };
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
        active_source,
        &scoreboard_scope,
        now,
        || -> Result<Vec<Matchup>, String> {
            if oauth_available {
                source
                    .scoreboard(league_key, current_week)
                    .map_err(|error| error.to_string())
            } else {
                ensure_redzone_feed(
                    public_client,
                    &mut redzone_feed,
                    public_league_id.expect("public league id when !oauth_available"),
                    league_key,
                )
                .map(|feed| feed.matchups.clone())
                .map_err(|error| error.to_string())
            }
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
    let (mut mine, mine_stale, mine_source) = cached_or_fetch_any_source_at(
        store,
        "match_roster",
        active_source,
        &mine_scope,
        now,
        || -> Result<RosterWeekStats, String> {
            if oauth_available {
                source
                    .roster_week_stats(&matchup.teams[my_index].team_key, week)
                    .map_err(|error| error.to_string())
            } else {
                ensure_redzone_feed(
                    public_client,
                    &mut redzone_feed,
                    public_league_id.expect("public league id when !oauth_available"),
                    league_key,
                )
                .map_err(|error| error.to_string())
                .and_then(|feed| {
                    feed.roster_week_stats
                        .get(&matchup.teams[my_index].team_key)
                        .cloned()
                        .ok_or_else(|| "no roster available for the selected team".into())
                })
            }
        },
    )?;
    let (mut opponent, opponent_stale, opponent_source) = cached_or_fetch_any_source_at(
        store,
        "match_roster",
        active_source,
        &opponent_scope,
        now,
        || -> Result<RosterWeekStats, String> {
            if oauth_available {
                source
                    .roster_week_stats(&matchup.teams[opponent_index].team_key, week)
                    .map_err(|error| error.to_string())
            } else {
                ensure_redzone_feed(
                    public_client,
                    &mut redzone_feed,
                    public_league_id.expect("public league id when !oauth_available"),
                    league_key,
                )
                .map_err(|error| error.to_string())
                .and_then(|feed| {
                    feed.roster_week_stats
                        .get(&matchup.teams[opponent_index].team_key)
                        .cloned()
                        .ok_or_else(|| "no roster available for the opponent team".into())
                })
            }
        },
    )?;
    // The daily overlay needs MLBAM-identity-reconciled players, which only
    // an actual OAuth `sync` produces (never a `pp`/public-feed row) — under
    // freshest-wins, that has to be checked per fetch, not just once via
    // `oauth_available`, since a `public_pull` row can still win the
    // roster-cache read even when OAuth is available.
    let fully_reconciled = mine_source == YAHOO_SOURCE && opponent_source == YAHOO_SOURCE;
    let daily_date = if !force_weekly && fully_reconciled {
        Some(utc_date(SystemTime::now())?)
    } else {
        None
    };
    if let Some(day) = &daily_date {
        apply_daily_stats(
            store,
            league_key,
            &mut mine,
            &mut opponent,
            day,
            http.clone(),
        )?;
    }
    let odds = acquire_odds_context(store, http).unwrap_or_default();
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
        client
            .fetch_redzone(public_league_id, league_key)
            .map_err(|error| contextual("fetch public matchup feed", error).to_string())
    });
    result
        .as_ref()
        .map_err(|message| MatchupError(message.clone()))
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
    output.push_str(&table_heading("ADVICE", mode));
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
                let retry = if write_source == PUBLIC_PULL_SOURCE {
                    "run b9 pp"
                } else {
                    "run b9 sync"
                };
                Err(contextual(
                    "refresh data",
                    format!("{error}; {retry}, verify connectivity, and retry"),
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
        clean_fantasy_team_name(&mine.name),
        mine.wins,
        mine.losses,
        mine.ties,
        clean_fantasy_team_name(&opponent.name),
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
        output.push_str(&table_heading("ODDS", mode));
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
        title(&clean_fantasy_team_name(&view.team_name), mode)
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
    output.push_str(&table_heading(heading, mode));
    output.push('\n');
    for player in players.iter().filter(|player| player.position_type == role) {
        output.push_str(&format!(
            "  {:<3} {} ({})\n",
            player.slot_position, player.name, player.team
        ));
    }
}

fn acquire_odds_context(
    store: &mut Store,
    http: Arc<HttpClient>,
) -> Result<Vec<String>, MatchupError> {
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
    output.push_str(&table_heading("CATEGORIES", mode));
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
    _heading: &str,
    mine: &[PlayerWeekStats],
    opponent: &[PlayerWeekStats],
    role: &str,
    mode: HelpColorMode,
) {
    output.push('\n');
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
            table_heading(&format!("{:<20}", "HITTER"), mode),
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
            table_heading(&format!("{:<20}", "PITCHER"), mode),
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
            .unwrap_or_else(|| " ".repeat(67));
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
}

fn matchup_player_cell(player: &PlayerWeekStats, role: &str, mode: HelpColorMode) -> String {
    let name = format!("{} {}", player.name, player.team);
    let name = format!("{:<20}", name.chars().take(20).collect::<String>());
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
    let row = format!(
        "{name}{:<17}{stats}",
        status.chars().take(17).collect::<String>()
    );
    if player.slot_position == Position::InjuredList || player.injury_status.starts_with("IL") {
        warning(&row, mode)
    } else if player.slot_position == Position::Bench {
        dim(&row, mode)
    } else {
        row
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FantasyPlayer, FantasyTeam};
    use crate::providers::mlb::{BulkHittingSplit, BulkPitchingSplit, HittingStats, PitchingStats};
    use crate::transport::{ExecutorError, HttpExecutor, HttpResponse, ValidatedRequest};
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

    #[derive(Default)]
    struct FakeFantasySource {
        team_key: Option<String>,
        scoreboard: Option<Vec<Matchup>>,
        rosters: HashMap<String, RosterWeekStats>,
    }

    impl YahooFantasySource for FakeFantasySource {
        fn user_leagues(
            &self,
        ) -> Result<
            Vec<crate::providers::yahoo_fantasy::UserLeague>,
            crate::providers::yahoo_fantasy::YahooFantasyError,
        > {
            Err(crate::providers::yahoo_fantasy::YahooFantasyError::Incomplete("not used in test"))
        }
        fn team_key(
            &self,
            _league_key: &str,
        ) -> Result<String, crate::providers::yahoo_fantasy::YahooFantasyError> {
            self.team_key.clone().ok_or(
                crate::providers::yahoo_fantasy::YahooFantasyError::Incomplete("not used in test"),
            )
        }
        fn league_settings(
            &self,
            _league_key: &str,
        ) -> Result<
            crate::providers::yahoo_fantasy::LeagueSettings,
            crate::providers::yahoo_fantasy::YahooFantasyError,
        > {
            Err(crate::providers::yahoo_fantasy::YahooFantasyError::Incomplete("not used in test"))
        }
        fn standings(
            &self,
            _league_key: &str,
        ) -> Result<Vec<FantasyTeam>, crate::providers::yahoo_fantasy::YahooFantasyError> {
            Err(crate::providers::yahoo_fantasy::YahooFantasyError::Incomplete("not used in test"))
        }
        fn league_rosters(
            &self,
            _league_key: &str,
        ) -> Result<
            crate::providers::yahoo_fantasy::LeagueRosters,
            crate::providers::yahoo_fantasy::YahooFantasyError,
        > {
            Err(crate::providers::yahoo_fantasy::YahooFantasyError::Incomplete("not used in test"))
        }
        fn free_agents(
            &self,
            _league_key: &str,
        ) -> Result<Vec<FantasyPlayer>, crate::providers::yahoo_fantasy::YahooFantasyError>
        {
            Err(crate::providers::yahoo_fantasy::YahooFantasyError::Incomplete("not used in test"))
        }
        fn scoreboard(
            &self,
            _league_key: &str,
            _week: Option<i32>,
        ) -> Result<Vec<Matchup>, crate::providers::yahoo_fantasy::YahooFantasyError> {
            self.scoreboard.clone().ok_or(
                crate::providers::yahoo_fantasy::YahooFantasyError::Incomplete("not used in test"),
            )
        }
        fn roster_week_stats(
            &self,
            team_key: &str,
            _week: i32,
        ) -> Result<RosterWeekStats, crate::providers::yahoo_fantasy::YahooFantasyError> {
            self.rosters.get(team_key).cloned().ok_or(
                crate::providers::yahoo_fantasy::YahooFantasyError::Incomplete("not used in test"),
            )
        }
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

        // Re-resolving to the same team reports no change, matching `pp
        // -l`'s persist-once-then-reuse pattern for the caller's write gate.
        let (resolved_again, changed_again) =
            resolve_team_override(&teams, &mut config, "vladdy").unwrap();
        assert_eq!(resolved_again, "l.1.t.2");
        assert!(!changed_again);

        assert!(resolve_team_override(&teams, &mut config, "nobody").is_err());
    }

    #[test]
    fn weekly_matchup_fetches_the_public_feed_and_caches_then_reuses_it_without_a_second_fetch() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let public = public_client(vec![ok_response()]);
        let source = FakeFantasySource::default();
        let now = SystemTime::now();

        let first = show_weekly_matchup(
            &mut store,
            "469.l.170874",
            Some("469.l.170874.t.1".into()),
            false,
            &source,
            &public,
            Some("170874"),
            false,
            failing_http_client(),
            now,
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
            false,
            &source,
            &empty_public,
            Some("170874"),
            false,
            failing_http_client(),
            now + Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn weekly_matchup_falls_back_to_the_stale_snapshot_when_a_later_fetch_fails() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let source = FakeFantasySource::default();
        let now = SystemTime::now();

        show_weekly_matchup(
            &mut store,
            "469.l.170874",
            Some("469.l.170874.t.1".into()),
            false,
            &source,
            &public_client(vec![ok_response()]),
            Some("170874"),
            false,
            failing_http_client(),
            now,
        )
        .unwrap();

        // Past the TTL, so the failing fetch is actually attempted instead
        // of short-circuiting on the still-fresh cache.
        let later = now + MATCHUP_TTL + Duration::from_secs(1);
        let fallback = show_weekly_matchup(
            &mut store,
            "469.l.170874",
            Some("469.l.170874.t.1".into()),
            false,
            &source,
            &public_client(vec![blocked_response()]),
            Some("170874"),
            false,
            failing_http_client(),
            later,
        )
        .unwrap();
        assert!(fallback.contains("STALE"));
        assert!(fallback.contains("New York Yankees"));
    }

    #[test]
    fn weekly_matchup_with_no_prior_snapshot_reports_the_fetch_error() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let source = FakeFantasySource::default();
        let error = show_weekly_matchup(
            &mut store,
            "469.l.170874",
            Some("469.l.170874.t.1".into()),
            false,
            &source,
            &public_client(vec![blocked_response()]),
            Some("170874"),
            false,
            failing_http_client(),
            SystemTime::now(),
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("403"));
        // A pp-only user has no OAuth login to run `b9 sync` against — the
        // recovery hint must point at `b9 pp`, not the OAuth-only command.
        assert!(message.contains("run b9 pp"));
        assert!(!message.contains("run b9 sync"));
    }

    #[test]
    fn weekly_matchup_prefers_a_fresher_public_pull_row_even_when_oauth_is_available() {
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

        // An OAuth source that errors if actually called — proves the
        // fresher public_pull cache is used instead of a live OAuth fetch.
        let source = FakeFantasySource::default();
        let output = show_weekly_matchup(
            &mut store,
            league_key,
            Some(team_key.into()),
            true,
            &source,
            &public_client(vec![]),
            None,
            false,
            failing_http_client(),
            SystemTime::now(),
        )
        .unwrap();
        assert!(output.contains("Team A"));
        assert!(output.contains("Team B"));
        // Even though OAuth is available, the roster data actually served
        // came from public_pull (unreconciled), so no daily overlay/label.
        assert!(!output.contains("DAY "));
    }

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
