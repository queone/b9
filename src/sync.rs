//! Foreground fantasy workflow application services.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{BufRead, Write};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config;
use crate::providers::yahoo::YahooClient;
use crate::providers::yahoo_fantasy::{YahooFantasyClient, YahooFantasyError, YahooFantasySource};
use crate::store::{
    CategoryWrite, FantasySnapshotWrite, IdentityCandidate, ItemRefreshPolicy, PositionWrite,
    SeasonStatWrite, Store, StoreStatus, SyncMode, SyncOrigin,
};
use crate::terminal::{self, HelpColorMode};
use crate::transport::HttpClient;

/// One user-facing fantasy workflow failure.
#[derive(Debug)]
pub struct WorkflowError(String);

impl WorkflowError {
    fn context(operation: &str, error: impl fmt::Display) -> Self {
        Self(format!("{operation}: {error}"))
    }
}

impl fmt::Display for WorkflowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for WorkflowError {}

fn production_yahoo() -> Result<Arc<YahooClient>, WorkflowError> {
    let http = Arc::new(
        HttpClient::production()
            .map_err(|error| WorkflowError::context("initialize HTTP transport", error))?,
    );
    YahooClient::production(http)
        .map(Arc::new)
        .map_err(|error| WorkflowError::context("initialize Yahoo", error))
}

/// Run interactive Yahoo login with explicit terminal and browser boundaries.
pub fn login_with(
    yahoo: &YahooClient,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    open_browser: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<(), WorkflowError> {
    let authorization = yahoo
        .begin_authorization()
        .map_err(|error| WorkflowError::context("start login", error))?;
    if let Err(error) = open_browser(&authorization.url) {
        writeln!(output, "Browser did not open ({error}).")
            .map_err(|error| WorkflowError::context("write login prompt", error))?;
    }
    writeln!(
        output,
        "Open this Yahoo authorization URL:\n{}",
        authorization.url
    )
    .map_err(|error| WorkflowError::context("write login URL", error))?;
    writeln!(output, "Paste the complete callback URL:")
        .map_err(|error| WorkflowError::context("write login prompt", error))?;
    let mut callback = String::new();
    input
        .read_line(&mut callback)
        .map_err(|error| WorkflowError::context("read callback URL", error))?;
    yahoo
        .complete_authorization(authorization.pending, callback.trim())
        .map_err(|error| WorkflowError::context("complete login", error))?;
    writeln!(output, "Yahoo login complete.")
        .map_err(|error| WorkflowError::context("write login result", error))?;
    Ok(())
}

/// Run production Yahoo login.
pub fn login() -> Result<(), WorkflowError> {
    let http = Arc::new(
        HttpClient::production()
            .map_err(|error| WorkflowError::context("initialize HTTP transport", error))?,
    );
    let yahoo = Arc::new(
        YahooClient::production(http.clone())
            .map_err(|error| WorkflowError::context("initialize Yahoo", error))?,
    );
    let mut input = std::io::BufReader::new(std::io::stdin());
    let mut output = std::io::stdout();
    let mut browser = |url: &str| {
        #[cfg(target_os = "macos")]
        let status = Command::new("open").arg(url).status();
        #[cfg(target_os = "windows")]
        let status = Command::new("cmd").args(["/C", "start", "", url]).status();
        #[cfg(all(unix, not(target_os = "macos")))]
        let status = Command::new("xdg-open").arg(url).status();
        status
            .map_err(|error| error.to_string())
            .and_then(|status| {
                status
                    .success()
                    .then_some(())
                    .ok_or_else(|| "browser command failed".into())
            })
    };
    login_with(&yahoo, &mut input, &mut output, &mut browser)
}

/// Remove the production Yahoo credential idempotently.
pub fn logout() -> Result<String, WorkflowError> {
    production_yahoo()?
        .delete_credential()
        .map_err(|error| WorkflowError::context("logout", error))?;
    Ok("Yahoo logout complete.\n".into())
}

/// Render local-first status without accessing Yahoo or the operating-system credential store.
pub fn status(requested_league: Option<&str>) -> Result<String, WorkflowError> {
    let mut config =
        config::read().map_err(|error| WorkflowError::context("read configuration", error))?;
    if let Some(selected) = requested_league.filter(|value| !value.trim().is_empty())
        && config.current_league != selected
    {
        config.current_league = selected.to_owned();
        config.current_team_key.clear();
        config::write(&config)
            .map_err(|error| WorkflowError::context("save league selection", error))?;
    }
    let database = crate::store::database_path()
        .map_err(|error| WorkflowError::context("resolve database status", error))?;
    let config_path = config::config_path()
        .map_err(|error| WorkflowError::context("resolve configuration path", error))?;
    let store_status = crate::store::inspect_status_at(&database, &config.current_league)
        .map_err(|error| WorkflowError::context("inspect database status", error))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default();
    Ok(render_dashboard(
        &database,
        &config_path,
        &config,
        &store_status,
        now,
        terminal::detected_help_color_mode(),
    ))
}

/// Format elapsed seconds as a fixed `HhMmSs` uptime string.
fn format_duration(seconds: u64) -> String {
    format!(
        "{}h {}m {}s",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

/// Render the settled, fixed-order `b9 st` dashboard.
///
/// Field order is contracted by AT3/AT8: service state and uptime, last/next
/// run and completion state, database path/size/schema, MLB/Yahoo identity
/// counts, provider freshness, circuit state and bounded last error,
/// unmatched-player count, then selected league/config paths.
pub fn render_dashboard(
    database_path: &Path,
    config_path: &Path,
    config: &config::Config,
    status: &StoreStatus,
    now: i64,
    mode: HelpColorMode,
) -> String {
    let has_snapshot = status.mlb_identity_count > 0 || status.unmatched_player_count > 0;
    let daemon_running = status.daemon_started_at.is_some() && status.daemon_stopped_at.is_none();

    let service = if daemon_running {
        let uptime = status
            .daemon_started_at
            .map_or(0, |started| (now - started).max(0) as u64);
        terminal::good(
            &format!("running (uptime {})", format_duration(uptime)),
            mode,
        )
    } else {
        terminal::dim("stopped", mode)
    };

    let last_run = match (&status.last_run_status, status.last_run_at) {
        (Some(run_status), Some(at)) if run_status == "success" => {
            terminal::good(&format!("{run_status} at unix {at}"), mode)
        }
        (Some(run_status), Some(at)) => {
            terminal::warning(&format!("{run_status} at unix {at}"), mode)
        }
        _ => terminal::dim("none", mode),
    };
    let next_run = if !daemon_running {
        terminal::dim("not scheduled (daemon stopped)", mode)
    } else {
        status.next_run_at.map_or_else(
            || terminal::dim("unavailable", mode),
            |at| format!("unix {at}"),
        )
    };

    let database = format!(
        "{} ({}, schema {})",
        database_path.display(),
        status
            .database_bytes
            .map_or_else(|| "absent".to_owned(), |bytes| format!("{bytes} bytes")),
        status
            .schema_version
            .map_or_else(|| "unknown".to_owned(), |version| format!("v{version}"))
    );

    let identities = if has_snapshot {
        format!(
            "{} MLB, {} Yahoo",
            status.mlb_identity_count, status.yahoo_identity_count
        )
    } else {
        terminal::dim("unavailable", mode)
    };

    let provider_freshness = if !has_snapshot {
        terminal::dim("unavailable", mode)
    } else {
        status
            .provider_freshness_at
            .map_or_else(|| terminal::dim("none", mode), |at| format!("unix {at}"))
    };

    let circuit = format!(
        "{} ({} failed requests)",
        if status.circuit_open {
            terminal::warning("open", mode)
        } else {
            terminal::good("closed", mode)
        },
        status.provider_failure_count
    );
    let last_error = status
        .provider_last_error
        .as_deref()
        .unwrap_or("none")
        .to_owned();

    let unmatched = if has_snapshot {
        status.unmatched_player_count.to_string()
    } else {
        terminal::dim("unavailable", mode)
    };

    let selected_league = if config.current_league.is_empty() {
        "none"
    } else {
        &config.current_league
    };

    let mut output = format!(
        "Yahoo: not checked (run b9 login or b9 sync)\n\
         Service: {service}\n\
         Last run: {last_run}\n\
         Next run: {next_run}\n\
         Database: {database}\n\
         Identities: {identities}\n\
         Provider freshness: {provider_freshness}\n\
         Circuit: {circuit}\n\
         Last provider error: {last_error}\n\
         Unmatched players: {unmatched}\n\
         League: {selected_league}\n\
         Config: {}\n",
        config_path.display()
    );
    if !has_snapshot {
        output.push_str("No local snapshot; run b9 sync.\n");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::render_dashboard;
    use crate::config::Config;
    use crate::store::StoreStatus;
    use crate::terminal::HelpColorMode;
    use std::path::Path;

    #[test]
    fn local_status_empty_snapshot_is_explicit_and_nonzero_free() {
        let output = render_dashboard(
            Path::new("/absent/b9.db"),
            Path::new("/absent/config.json"),
            &Config::default(),
            &StoreStatus::default(),
            0,
            HelpColorMode::Plain,
        );
        assert!(output.contains("Yahoo: not checked (run b9 login or b9 sync)"));
        assert!(output.contains("Service: stopped"));
        assert!(output.contains("Last run: none"));
        assert!(output.contains("Next run: not scheduled (daemon stopped)"));
        assert!(output.contains("Database: /absent/b9.db (absent, schema unknown)"));
        assert!(output.contains("Identities: unavailable"));
        assert!(output.contains("Provider freshness: unavailable"));
        assert!(output.contains("Circuit: closed (0 failed requests)"));
        assert!(output.contains("Unmatched players: unavailable"));
        assert!(output.contains("League: none"));
        assert!(output.contains("Config: /absent/config.json"));
        assert!(output.contains("No local snapshot; run b9 sync."));
        assert!(!output.contains("0 MLB"));
        assert!(!output.contains("Unmatched players: 0"));
    }

    #[test]
    fn populated_snapshot_reports_real_counts_without_the_no_snapshot_hint() {
        let status = StoreStatus {
            mlb_identity_count: 512,
            yahoo_identity_count: 480,
            unmatched_player_count: 6,
            provider_freshness_at: Some(100),
            daemon_started_at: Some(40),
            last_run_status: Some("success".into()),
            last_run_at: Some(100),
            next_run_at: Some(200),
            database_bytes: Some(1024),
            schema_version: Some(3),
            ..StoreStatus::default()
        };
        let config = Config {
            current_league: "431.l.12345".into(),
            ..Config::default()
        };
        let output = render_dashboard(
            Path::new("/srv/b9/.config/b9/b9.db"),
            Path::new("/srv/b9/.config/b9/config.json"),
            &config,
            &status,
            160,
            HelpColorMode::Plain,
        );
        assert!(output.contains("Service: running (uptime 0h 2m 0s)"));
        assert!(output.contains("Last run: success at unix 100"));
        assert!(output.contains("Next run: unix 200"));
        assert!(output.contains("Database: /srv/b9/.config/b9/b9.db (1024 bytes, schema v3)"));
        assert!(output.contains("Identities: 512 MLB, 480 Yahoo"));
        assert!(output.contains("Provider freshness: unix 100"));
        assert!(output.contains("Unmatched players: 6"));
        assert!(output.contains("League: 431.l.12345"));
        assert!(!output.contains("No local snapshot"));
    }
}

/// Choose a visible league through an injectable terminal boundary.
pub fn select_league(
    leagues: &[crate::providers::yahoo_fantasy::UserLeague],
    requested: Option<&str>,
    interactive: bool,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<Option<String>, WorkflowError> {
    if let Some(key) = requested {
        return leagues
            .iter()
            .find(|league| league.league_key == key)
            .map(|league| Some(league.league_key.clone()))
            .ok_or_else(|| WorkflowError("select league: key is not visible to the authenticated user; run b9 st and retry".into()));
    }
    match leagues {
        [] => Ok(None),
        [league] => Ok(Some(league.league_key.clone())),
        _ if !interactive => Err(WorkflowError(
            "select league: multiple leagues found; run b9 st -l <key> and retry".into(),
        )),
        _ => {
            writeln!(output, "Select a league:")
                .map_err(|error| WorkflowError::context("write league selection", error))?;
            for (index, league) in leagues.iter().enumerate() {
                writeln!(
                    output,
                    "  {}. {}  {}",
                    index + 1,
                    league.league_key,
                    league.name
                )
                .map_err(|error| WorkflowError::context("write league selection", error))?;
            }
            write!(output, "Choice: ")
                .map_err(|error| WorkflowError::context("write league selection", error))?;
            let mut choice = String::new();
            input
                .read_line(&mut choice)
                .map_err(|error| WorkflowError::context("read league selection", error))?;
            let index = choice
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| {
                    WorkflowError(
                        "select league: enter one of the displayed numbers and retry".into(),
                    )
                })?;
            leagues
                .get(index - 1)
                .map(|league| Some(league.league_key.clone()))
                .ok_or_else(|| {
                    WorkflowError(
                        "select league: enter one of the displayed numbers and retry".into(),
                    )
                })
        }
    }
}

/// Deterministic counts returned by one complete foreground synchronization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncSummary {
    pub team_key: String,
    pub teams: usize,
    pub players: usize,
    pub roster_slots: usize,
    pub mlb_identities: usize,
}

/// Synchronize through injected provider, store, and optional identity boundaries.
pub fn synchronize_with(
    source: &dyn YahooFantasySource,
    store: &mut Store,
    league_key: &str,
    identities_for_season: &mut dyn FnMut(i32) -> Vec<IdentityCandidate>,
) -> Result<SyncSummary, WorkflowError> {
    synchronize_with_origin(
        source,
        store,
        league_key,
        SyncOrigin::Manual,
        identities_for_season,
    )
}

/// Synchronize through injected boundaries with an explicit durable caller origin.
pub fn synchronize_with_origin(
    source: &dyn YahooFantasySource,
    store: &mut Store,
    league_key: &str,
    origin: SyncOrigin,
    identities_for_season: &mut dyn FnMut(i32) -> Vec<IdentityCandidate>,
) -> Result<SyncSummary, WorkflowError> {
    let run = store
        .start_sync_run(SyncMode::Live, origin)
        .map_err(|error| WorkflowError::context("start sync run", error))?;
    let result = (|| {
        let settings = source
            .league_settings(league_key)
            .map_err(|error| WorkflowError::context("sync league settings", error))?;
        let teams = source
            .standings(league_key)
            .map_err(|error| WorkflowError::context("sync standings", error))?;
        let rosters = source
            .league_rosters(league_key)
            .map_err(|error| WorkflowError::context("sync rosters", error))?;
        let free_agents = source
            .free_agents(league_key)
            .map_err(|error| WorkflowError::context("sync free agents", error))?;
        let team_key = source
            .team_key(league_key)
            .map_err(|error| WorkflowError::context("resolve authenticated team", error))?;
        if !teams.iter().any(|team| team.team_key == team_key) {
            return Err(WorkflowError("sync: authenticated team is outside the complete standings; prior data was retained".into()));
        }
        let snapshot = FantasySnapshotWrite {
            league: settings.league,
            current_week: settings.current_week,
            categories: settings
                .categories
                .into_iter()
                .map(|row| CategoryWrite {
                    stat_id: row.stat_id,
                    abbreviation: row.abbreviation,
                    name: row.name,
                    sort_order: row.sort_order,
                    display_only: row.display_only,
                    sequence: row.sequence,
                })
                .collect(),
            positions: settings
                .roster_positions
                .into_iter()
                .map(|row| PositionWrite {
                    position: row.position.to_string(),
                    count: row.count,
                })
                .collect(),
            teams,
            players: rosters.players.into_iter().chain(free_agents).collect(),
            slots: rosters.slots,
        };
        store
            .replace_fantasy_snapshot(&snapshot)
            .map_err(|error| WorkflowError::context("persist fantasy snapshot", error))?;
        let reconciled = store
            .reconcile_mlb_identities(&identities_for_season(snapshot.league.season))
            .map_err(|error| WorkflowError::context("reconcile MLB identities", error))?;
        let summary = SyncSummary {
            team_key,
            teams: snapshot.teams.len(),
            players: snapshot.players.len(),
            roster_slots: snapshot.slots.len(),
            mlb_identities: reconciled,
        };
        let mut counts = BTreeMap::new();
        counts.insert("mlb_identities".into(), summary.mlb_identities as i64);
        counts.insert("players".into(), summary.players as i64);
        counts.insert("roster_slots".into(), summary.roster_slots as i64);
        counts.insert("teams".into(), summary.teams as i64);
        store
            .complete_sync_run(run, &counts)
            .map_err(|error| WorkflowError::context("complete sync run", error))?;
        store
            .record_provider_success()
            .map_err(|error| WorkflowError::context("record provider success", error))?;
        Ok(summary)
    })();
    if let Err(error) = &result {
        let _ = store.fail_sync_run(run);
        let _ = store.record_provider_failure(&error.to_string());
    }
    result
}

/// Synchronize the selected league's stable normalized Yahoo data in the foreground.
pub fn synchronize(league_override: Option<&str>) -> Result<String, WorkflowError> {
    synchronize_with_options(league_override, false, false)
}

/// Synchronize manually, optionally bypassing all application freshness gates.
pub fn synchronize_with_options(
    league_override: Option<&str>,
    _force: bool,
    include_authenticated: bool,
) -> Result<String, WorkflowError> {
    // Stable Yahoo synchronization currently always acquires a complete snapshot. Keeping force
    // explicit here preserves manual execution semantics when freshness gates are introduced.
    synchronize_for_origin_reporting(
        league_override,
        SyncOrigin::Manual,
        &mut |_| Ok(()),
        true,
        include_authenticated,
    )
}

/// Synchronize manually while writing and flushing each completed step immediately.
pub fn synchronize_with_options_streaming(
    league_override: Option<&str>,
    _force: bool,
    include_authenticated: bool,
    output: &mut dyn Write,
) -> Result<String, WorkflowError> {
    let mut reporter = |line: &str| write_sync_progress(output, line);
    synchronize_for_origin_reporting(
        league_override,
        SyncOrigin::Manual,
        &mut reporter,
        false,
        include_authenticated,
    )
}

/// Synchronize the selected league through the shared production service.
pub fn synchronize_for_origin(
    league_override: Option<&str>,
    origin: SyncOrigin,
) -> Result<String, WorkflowError> {
    synchronize_for_origin_reporting(league_override, origin, &mut |_| Ok(()), true, false)
}

fn synchronize_for_origin_reporting(
    league_override: Option<&str>,
    origin: SyncOrigin,
    reporter: &mut dyn FnMut(&str) -> Result<(), WorkflowError>,
    include_step_lines: bool,
    include_authenticated: bool,
) -> Result<String, WorkflowError> {
    let _guard = crate::daemon::SyncGuard::acquire()
        .map_err(|error| WorkflowError::context("start synchronization", error))?;
    let mut config =
        config::read().map_err(|error| WorkflowError::context("read configuration", error))?;
    let original_config = config.clone();
    let mut league_key = league_override
        .filter(|key| !key.trim().is_empty())
        .unwrap_or(&config.current_league)
        .to_owned();
    if league_key.is_empty() && !config.pull_public_league_id.is_empty() {
        league_key = format!("public.{}", config.pull_public_league_id);
        config.current_league = league_key.clone();
    }
    let http = Arc::new(
        HttpClient::production()
            .map_err(|error| WorkflowError::context("initialize HTTP transport", error))?,
    );
    let mut store =
        Store::open().map_err(|error| WorkflowError::context("open database", error))?;
    let run = store
        .start_sync_run(SyncMode::Live, origin)
        .map_err(|error| WorkflowError::context("start sync run", error))?;
    let date = sync_utc_date(SystemTime::now())?;
    let season = store
        .fantasy_season(&league_key)
        .map_err(|error| WorkflowError::context("read sync season", error))?
        .unwrap_or_else(|| date[..4].parse().unwrap_or(1970));
    let force = origin == SyncOrigin::Manual;
    let mut outcomes = Vec::new();

    let public_scope = if league_key.is_empty() {
        "unconfigured"
    } else {
        &league_key
    };
    let public = run_sync_item(
        &mut store,
        "yahoo_public",
        "redzone",
        public_scope,
        origin,
        force,
        |store| {
            let league_id = crate::providers::yahoo_public::league_id_from_key(&league_key)
                .or_else(|_| {
                    crate::providers::yahoo_public::league_id_from_key(
                        &config.pull_public_league_id,
                    )
                })
                .map_err(|error| {
                    format!("resolve public Yahoo league: {error}; configure a league and retry")
                })?;
            let client = crate::providers::yahoo_public::YahooPublicClient::shared(http.clone());
            let mut feed = client
                .fetch_redzone(&league_id, &league_key)
                .map_err(|error| format!("fetch public Yahoo snapshot: {error}; retry later"))?;
            client
                .enrich_team_transactions(&league_key, &mut feed.teams)
                .map_err(|error| {
                    format!("fetch public Yahoo team transactions: {error}; retry later")
                })?;
            client
                .enrich_player_ranks(&mut feed.players)
                .map_err(|error| {
                    format!("fetch public Yahoo player ranks: {error}; retry later")
                })?;
            let snapshot = crate::public_pull::public_snapshot(feed);
            let count = snapshot.players.len() as i64;
            store.merge_public_fantasy_snapshot(&snapshot)
                .map_err(|error| format!("persist public Yahoo snapshot: {error}; prior supplemental data was retained"))?;
            Ok(count)
        },
    );
    record_outcome(&mut outcomes, public, reporter)?;

    let mut authenticated_outcomes = Vec::new();
    if !include_authenticated {
        // Public Yahoo is the default and does not consult Keychain.
    } else if league_key.is_empty() {
        record_outcome(
            &mut authenticated_outcomes,
            failed_sync_item(
                &mut store,
                "yahoo_authenticated",
                "configuration",
                public_scope,
                "authenticated Yahoo league is not configured; run b9 login when API access is available",
            ),
            reporter,
        )?;
    } else {
        let yahoo = YahooClient::production(http.clone()).map(Arc::new);
        match yahoo {
            Ok(yahoo) => {
                let source = YahooFantasyClient::new(yahoo);
                let mut terminal_failures = 0;
                record_outcome(
                    &mut authenticated_outcomes,
                    run_sync_item(
                        &mut store,
                        "yahoo_authenticated",
                        "settings",
                        public_scope,
                        origin,
                        force,
                        |store| {
                            let settings =
                                source.league_settings(&league_key).map_err(|error| {
                                    authenticated_yahoo_error(
                                        "fetch authenticated Yahoo settings",
                                        error,
                                        &mut terminal_failures,
                                    )
                                })?;
                            let rows = settings
                                .categories
                                .into_iter()
                                .map(|row| CategoryWrite {
                                    stat_id: row.stat_id,
                                    abbreviation: row.abbreviation,
                                    name: row.name,
                                    sort_order: row.sort_order,
                                    display_only: row.display_only,
                                    sequence: row.sequence,
                                })
                                .collect::<Vec<_>>();
                            store
                                .replace_authenticated_categories(&league_key, &rows)
                                .map_err(|error| {
                                    format!("persist authenticated Yahoo settings: {error}")
                                })?;
                            Ok(rows.len() as i64)
                        },
                    ),
                    reporter,
                )?;
                record_outcome(
                    &mut authenticated_outcomes,
                    run_sync_item(
                        &mut store,
                        "yahoo_authenticated",
                        "standings",
                        public_scope,
                        origin,
                        force,
                        |store| {
                            let rows = source.standings(&league_key).map_err(|error| {
                                authenticated_yahoo_error(
                                    "fetch authenticated Yahoo standings",
                                    error,
                                    &mut terminal_failures,
                                )
                            })?;
                            store.merge_authenticated_teams(&rows).map_err(|error| {
                                format!("persist authenticated Yahoo standings: {error}")
                            })?;
                            Ok(rows.len() as i64)
                        },
                    ),
                    reporter,
                )?;
                record_outcome(
                    &mut authenticated_outcomes,
                    run_sync_item(
                        &mut store,
                        "yahoo_authenticated",
                        "rosters",
                        public_scope,
                        origin,
                        force,
                        |store| {
                            let rows = source.league_rosters(&league_key).map_err(|error| {
                                authenticated_yahoo_error(
                                    "fetch authenticated Yahoo rosters",
                                    error,
                                    &mut terminal_failures,
                                )
                            })?;
                            store
                                .merge_authenticated_players(&rows.players)
                                .map_err(|error| {
                                    format!("persist authenticated Yahoo roster metadata: {error}")
                                })?;
                            Ok(rows.players.len() as i64)
                        },
                    ),
                    reporter,
                )?;
                let free_agents = if authenticated_yahoo_limit_reached(terminal_failures) {
                    skipped_sync_item(
                        &mut store,
                        "yahoo_authenticated",
                        "free_agents",
                        public_scope,
                    )
                } else {
                    run_sync_item(
                        &mut store,
                        "yahoo_authenticated",
                        "free_agents",
                        public_scope,
                        origin,
                        force,
                        |store| {
                            let rows = source.free_agents(&league_key).map_err(|error| {
                                authenticated_yahoo_error(
                                    "fetch authenticated Yahoo free agents",
                                    error,
                                    &mut terminal_failures,
                                )
                            })?;
                            store
                                .replace_authenticated_free_agents(&league_key, &rows)
                                .map_err(|error| {
                                    format!("persist authenticated Yahoo free agents: {error}")
                                })?;
                            Ok(rows.len() as i64)
                        },
                    )
                };
                record_outcome(&mut authenticated_outcomes, free_agents, reporter)?;
                let team_identity = if authenticated_yahoo_limit_reached(terminal_failures) {
                    skipped_sync_item(
                        &mut store,
                        "yahoo_authenticated",
                        "team_identity",
                        public_scope,
                    )
                } else {
                    run_sync_item(
                        &mut store,
                        "yahoo_authenticated",
                        "team_identity",
                        public_scope,
                        origin,
                        force,
                        |_| {
                            let key = source.team_key(&league_key).map_err(|error| {
                                authenticated_yahoo_error(
                                    "resolve authenticated Yahoo team",
                                    error,
                                    &mut terminal_failures,
                                )
                            })?;
                            if league_override.is_none() || config.current_league == league_key {
                                config.current_team_key = key;
                            }
                            Ok(1)
                        },
                    )
                };
                record_outcome(&mut authenticated_outcomes, team_identity, reporter)?;
            }
            Err(error) => {
                record_outcome(
                    &mut authenticated_outcomes,
                    failed_sync_item(
                        &mut store,
                        "yahoo_authenticated",
                        "initialization",
                        public_scope,
                        &format!(
                            "initialize authenticated Yahoo: {error}; public Yahoo remains available"
                        ),
                    ),
                    reporter,
                )?;
            }
        }
    }
    if include_authenticated
        && authenticated_outcomes
            .iter()
            .all(|outcome| outcome.succeeded)
    {
        let _ = store.record_provider_success();
    } else if include_authenticated
        && let Some(failure) = authenticated_outcomes
            .iter()
            .find(|outcome| !outcome.succeeded)
    {
        let _ = store.record_provider_failure(&failure.detail);
    }
    outcomes.extend(authenticated_outcomes);

    let mlb = crate::providers::mlb::MlbClient::production(http.clone());
    let mlb_hitting = run_sync_item(
        &mut store,
        "mlb",
        "hitting",
        &season.to_string(),
        origin,
        force,
        |store| {
            let rows = mlb.fetch_bulk_hitting_stats(season, "R").map_err(|error| {
                format!("fetch MLB hitting: {error}; prior hitting data was retained")
            })?;
            let writes = rows.iter().map(hitting_write).collect::<Vec<_>>();
            store
                .replace_mlb_season_stats(season, &writes)
                .map_err(|error| {
                    format!("persist MLB hitting: {error}; prior hitting data was retained")
                })?;
            let identities = rows
                .into_iter()
                .map(|row| IdentityCandidate {
                    mlbam_id: row.player.person_id,
                    name: row.player.full_name,
                    team: team_abbreviation(row.team.team_id).to_owned(),
                    role: "B".into(),
                })
                .collect::<Vec<_>>();
            store
                .reconcile_mlb_identities(&identities)
                .map_err(|error| format!("reconcile MLB hitting identities: {error}"))?;
            Ok(writes.len() as i64)
        },
    );
    record_outcome(&mut outcomes, mlb_hitting, reporter)?;
    let mlb_pitching = run_sync_item(
        &mut store,
        "mlb",
        "pitching",
        &season.to_string(),
        origin,
        force,
        |store| {
            let rows = mlb
                .fetch_bulk_pitching_stats(season, "R")
                .map_err(|error| {
                    format!("fetch MLB pitching: {error}; prior pitching data was retained")
                })?;
            let pitcher_ids = rows
                .iter()
                .filter(|row| row.stat.games_started > 0)
                .map(|row| row.player.person_id)
                .collect::<Vec<_>>();
            let quality_starts =
                mlb.fetch_quality_starts(season, &pitcher_ids)
                    .map_err(|error| {
                        format!(
                            "fetch MLB quality starts: {error}; prior pitching data was retained"
                        )
                    })?;
            let mut writes = rows.iter().map(pitching_write).collect::<Vec<_>>();
            merge_quality_start_writes(&mut writes, &quality_starts.counts);
            store
                .replace_mlb_season_stats(season, &writes)
                .map_err(|error| {
                    format!("persist MLB pitching: {error}; prior pitching data was retained")
                })?;
            let identities = rows
                .into_iter()
                .map(|row| IdentityCandidate {
                    mlbam_id: row.player.person_id,
                    name: row.player.full_name,
                    team: team_abbreviation(row.team.team_id).to_owned(),
                    role: "P".into(),
                })
                .collect::<Vec<_>>();
            store
                .reconcile_mlb_identities(&identities)
                .map_err(|error| format!("reconcile MLB pitching identities: {error}"))?;
            Ok(writes.len() as i64)
        },
    );
    record_outcome(&mut outcomes, mlb_pitching, reporter)?;

    for historical_season in (season - 5)..season {
        for group in ["hitting", "pitching"] {
            let source = format!("mlbam_{group}");
            let complete = store
                .is_season_complete(&source, historical_season, 1)
                .map_err(|error| WorkflowError::context("read historical MLB manifest", error))?;
            if complete {
                record_outcome(
                    &mut outcomes,
                    SyncItemOutcome {
                        source: "mlb_history".into(),
                        item: format!("{historical_season}_{group}"),
                        succeeded: true,
                        skipped: true,
                        degraded: false,
                        count: 0,
                        detail: String::new(),
                    },
                    reporter,
                )?;
                continue;
            }
            let minimum = if group == "hitting" { 200 } else { 150 };
            let outcome = run_sync_item(
                &mut store,
                "mlb_history",
                &format!("{historical_season}_{group}"),
                &historical_season.to_string(),
                origin,
                true,
                |store| {
                    let writes = if group == "hitting" {
                        match mlb.fetch_bulk_hitting_stats(historical_season, "R") {
                            Ok(rows) => rows.iter().map(hitting_write).collect::<Vec<_>>(),
                            Err(error) => {
                                let _ = store.mark_season_failed(&source, historical_season, 0, 1);
                                return Err(format!(
                                    "fetch historical MLB {group}: {error}; prior rows were retained"
                                ));
                            }
                        }
                    } else {
                        match mlb.fetch_bulk_pitching_stats(historical_season, "R") {
                            Ok(rows) => rows.iter().map(pitching_write).collect::<Vec<_>>(),
                            Err(error) => {
                                let _ = store.mark_season_failed(&source, historical_season, 0, 1);
                                return Err(format!(
                                    "fetch historical MLB {group}: {error}; prior rows were retained"
                                ));
                            }
                        }
                    };
                    let count = writes.len() as i64;
                    if count < minimum {
                        let _ = store.mark_season_partial(&source, historical_season, count, 1);
                        return Err(format!(
                            "historical MLB {group} returned {count} rows below the {minimum}-row completeness minimum; prior rows were retained"
                        ));
                    }
                    if let Err(error) = store.replace_mlb_season_stats(historical_season, &writes) {
                        let _ = store.mark_season_failed(&source, historical_season, count, 1);
                        return Err(format!(
                            "persist historical MLB {group}: {error}; prior rows were retained"
                        ));
                    }
                    store
                        .mark_season_complete(&source, historical_season, count, 1)
                        .map_err(|error| {
                            format!("complete historical MLB {group} manifest: {error}")
                        })?;
                    Ok(count)
                },
            );
            record_outcome(&mut outcomes, outcome, reporter)?;
        }
    }

    let roster_outcome = run_sync_item(
        &mut store,
        "mlb",
        "40man_rosters",
        &season.to_string(),
        origin,
        force,
        |store| {
            let teams = mlb.fetch_team_directory(season).map_err(|error| {
                format!("fetch MLB roster directory: {error}; all prior team rosters were retained")
            })?;
            let identities = teams
                .iter()
                .map(|team| (team.team_id, team.abbreviation.clone()))
                .collect::<Vec<_>>();
            sync_mlb_rosters(store, &identities, |team_id| {
                mlb.fetch_roster(team_id)
                    .map_err(|error| error.to_string())
                    .map(|rows| {
                        rows.into_iter()
                            .map(|row| crate::store::RosterWrite {
                                mlbam_id: row.person_id,
                                name: row.full_name,
                                position: row.position,
                                primary_type: match row.primary_type {
                                    crate::providers::mlb::PrimaryType::H => "H",
                                    crate::providers::mlb::PrimaryType::P => "P",
                                }
                                .into(),
                                status: row.status,
                                jersey_number: row.jersey_number,
                            })
                            .collect()
                    })
            })
        },
    );
    record_outcome(&mut outcomes, roster_outcome, reporter)?;

    let savant = crate::providers::savant::SavantClient::production(http.clone());
    for group in ["batting", "pitching"] {
        record_outcome(
            &mut outcomes,
            run_sync_item(
                &mut store,
                "savant",
                group,
                &season.to_string(),
                origin,
                force,
                |store| {
                    let rows = if group == "batting" {
                        savant.fetch_batting(season)
                    } else {
                        savant.fetch_pitching(season)
                    }
                    .map_err(|error| {
                        format!("fetch Savant {group}: {error}; prior {group} data was retained")
                    })?;
                    let count = rows.len() as i64;
                    store
                        .replace_statcast_snapshot(season, group, &rows)
                        .map_err(|error| {
                            format!(
                                "persist Savant {group}: {error}; prior {group} data was retained"
                            )
                        })?;
                    Ok(count)
                },
            ),
            reporter,
        )?;
    }

    let espn_outcome = run_sync_item(
        &mut store,
        "espn",
        "mlb_current_odds",
        &date,
        origin,
        force,
        |store| {
            let slate = crate::providers::espn::EspnClient::production(http.clone())
                .fetch_game_lines(SystemTime::now())
                .map_err(|error| {
                    format!("fetch ESPN odds: {error}; prior odds snapshot was retained")
                })?;
            let payload = serde_json::to_string(&slate)
                .map_err(|error| format!("serialize ESPN odds: {error}"))?;
            store
                .save_command_snapshot("mlb_current_odds", "espn", &date, "1", &payload)
                .map_err(|error| {
                    format!("persist ESPN odds: {error}; prior odds snapshot was retained")
                })?;
            let count = slate.games.len() as i64;
            if slate.issues.is_empty() {
                Ok(SyncItemSuccess::complete(count))
            } else {
                Ok(SyncItemSuccess::degraded(
                    count,
                    format!("{} bounded odds issues", slate.issues.len()),
                ))
            }
        },
    );
    if !espn_outcome.succeeded {
        let _ = store.mark_command_snapshot_stale(
            "mlb_current_odds",
            "espn",
            &date,
            &espn_outcome.detail,
        );
    }
    record_outcome(&mut outcomes, espn_outcome, reporter)?;

    if config != original_config {
        config::write(&config)
            .map_err(|error| WorkflowError::context("save team identity", error))?;
    }
    let successes = outcomes.iter().filter(|outcome| outcome.succeeded).count();
    let failures = outcomes
        .iter()
        .filter(|outcome| !outcome.succeeded && !outcome.skipped)
        .count();
    let degradations = outcomes.iter().filter(|outcome| outcome.degraded).count();
    let mut counts = BTreeMap::new();
    for outcome in &outcomes {
        counts.insert(
            format!("{}_{}", outcome.source, outcome.item),
            outcome.count,
        );
    }
    if successes == 0 {
        let _ = store.fail_sync_run(run);
    } else {
        store
            .complete_sync_run(run, &counts)
            .map_err(|error| WorkflowError::context("complete sync run", error))?;
    }
    let step_output = outcomes
        .iter()
        .map(SyncItemOutcome::line)
        .collect::<Vec<_>>()
        .join("\n");
    if successes == 0 {
        let detail = if include_step_lines {
            format!("\n{step_output}")
        } else {
            String::new()
        };
        return Err(WorkflowError(format!(
            "sync failed: every provider failed{detail}\nCheck network access and provider credentials, then retry"
        )));
    }
    let disposition = if failures == 0 && degradations == 0 {
        "success"
    } else {
        "degraded success"
    };
    let aggregate =
        format!("Sync {disposition}: {successes} steps succeeded, {failures} failed.\n");
    let output = if include_step_lines {
        format!("{step_output}\n{aggregate}")
    } else {
        aggregate
    };
    Ok(output)
}

fn record_outcome(
    outcomes: &mut Vec<SyncItemOutcome>,
    outcome: SyncItemOutcome,
    reporter: &mut dyn FnMut(&str) -> Result<(), WorkflowError>,
) -> Result<(), WorkflowError> {
    reporter(&outcome.line())?;
    outcomes.push(outcome);
    Ok(())
}

fn write_sync_progress(output: &mut dyn Write, line: &str) -> Result<(), WorkflowError> {
    writeln!(output, "{line}")
        .and_then(|()| output.flush())
        .map_err(|error| WorkflowError::context("write sync progress", error))
}

const SYNC_PIPELINE_VERSION: &str = "provider-sync-v1";

struct SyncItemOutcome {
    source: String,
    item: String,
    succeeded: bool,
    skipped: bool,
    degraded: bool,
    count: i64,
    detail: String,
}

impl SyncItemOutcome {
    fn line(&self) -> String {
        let status = if self.skipped && !self.succeeded {
            "skipped"
        } else if self.succeeded {
            if self.skipped {
                "fresh"
            } else if self.degraded {
                "degraded"
            } else {
                "success"
            }
        } else {
            "failed"
        };
        if self.detail.is_empty() {
            format!("{} {}: {status} ({})", self.source, self.item, self.count)
        } else {
            format!("{} {}: {status}: {}", self.source, self.item, self.detail)
        }
    }
}

fn authenticated_yahoo_error(
    operation: &str,
    error: YahooFantasyError,
    terminal_failures: &mut usize,
) -> String {
    if error.is_terminal_access() {
        *terminal_failures += 1;
    }
    format!("{operation}: {error}")
}

fn authenticated_yahoo_limit_reached(terminal_failures: usize) -> bool {
    terminal_failures >= 3
}

fn skipped_sync_item(store: &mut Store, source: &str, item: &str, scope: &str) -> SyncItemOutcome {
    let detail = "terminal access failure limit reached after 3 attempts";
    let _ = store.mark_sync_item_failure(source, item, scope, SYNC_PIPELINE_VERSION, detail);
    SyncItemOutcome {
        source: source.into(),
        item: item.into(),
        succeeded: false,
        skipped: true,
        degraded: false,
        count: 0,
        detail: detail.into(),
    }
}

fn failed_sync_item(
    store: &mut Store,
    source: &str,
    item: &str,
    scope: &str,
    detail: &str,
) -> SyncItemOutcome {
    let _ = store.mark_sync_item_failure(source, item, scope, SYNC_PIPELINE_VERSION, detail);
    SyncItemOutcome {
        source: source.into(),
        item: item.into(),
        succeeded: false,
        skipped: false,
        degraded: false,
        count: 0,
        detail: detail.into(),
    }
}

struct SyncItemSuccess {
    count: i64,
    degraded: bool,
    detail: String,
}

impl SyncItemSuccess {
    fn complete(count: i64) -> Self {
        Self {
            count,
            degraded: false,
            detail: String::new(),
        }
    }

    fn degraded(count: i64, detail: String) -> Self {
        Self {
            count,
            degraded: true,
            detail,
        }
    }
}

impl From<i64> for SyncItemSuccess {
    fn from(count: i64) -> Self {
        Self::complete(count)
    }
}

fn run_sync_item<F, T>(
    store: &mut Store,
    source: &str,
    item: &str,
    scope: &str,
    origin: SyncOrigin,
    force: bool,
    action: F,
) -> SyncItemOutcome
where
    F: FnOnce(&mut Store) -> Result<T, String>,
    T: Into<SyncItemSuccess>,
{
    let policy = ItemRefreshPolicy {
        ttl: Duration::from_secs(30 * 60),
        force: force || origin == SyncOrigin::Manual,
        pipeline_version: SYNC_PIPELINE_VERSION.into(),
    };
    match store.needs_sync_item(source, item, scope, &policy) {
        Ok(true) => {}
        Ok(false) => {
            return SyncItemOutcome {
                source: source.into(),
                item: item.into(),
                succeeded: true,
                skipped: true,
                degraded: false,
                count: 0,
                detail: String::new(),
            };
        }
        Err(error) => {
            return SyncItemOutcome {
                source: source.into(),
                item: item.into(),
                succeeded: false,
                skipped: false,
                degraded: false,
                count: 0,
                detail: format!("evaluate provider freshness: {error}"),
            };
        }
    }
    if let Err(error) = store.mark_sync_item_attempt(source, item, scope, SYNC_PIPELINE_VERSION) {
        return SyncItemOutcome {
            source: source.into(),
            item: item.into(),
            succeeded: false,
            skipped: false,
            degraded: false,
            count: 0,
            detail: format!("record provider attempt: {error}"),
        };
    }
    match action(store) {
        Ok(success) => {
            let success = success.into();
            let state_result = if success.degraded {
                store.mark_sync_item_degraded(
                    source,
                    item,
                    scope,
                    SYNC_PIPELINE_VERSION,
                    &success.detail,
                )
            } else {
                store.mark_sync_item_success(source, item, scope, SYNC_PIPELINE_VERSION)
            };
            if let Err(error) = state_result {
                return SyncItemOutcome {
                    source: source.into(),
                    item: item.into(),
                    succeeded: false,
                    skipped: false,
                    degraded: false,
                    count: 0,
                    detail: format!("record provider success: {error}"),
                };
            }
            SyncItemOutcome {
                source: source.into(),
                item: item.into(),
                succeeded: true,
                skipped: false,
                degraded: success.degraded,
                count: success.count,
                detail: success.detail,
            }
        }
        Err(detail) => {
            let _ =
                store.mark_sync_item_failure(source, item, scope, SYNC_PIPELINE_VERSION, &detail);
            SyncItemOutcome {
                source: source.into(),
                item: item.into(),
                succeeded: false,
                skipped: false,
                degraded: false,
                count: 0,
                detail,
            }
        }
    }
}

fn sync_mlb_rosters<F>(
    store: &mut Store,
    teams: &[(i64, String)],
    mut fetch: F,
) -> Result<SyncItemSuccess, String>
where
    F: FnMut(i64) -> Result<Vec<crate::store::RosterWrite>, String>,
{
    let unique = teams
        .iter()
        .map(|(id, abbreviation)| (*id, abbreviation.as_str()))
        .collect::<std::collections::BTreeSet<_>>();
    if teams.len() != 30 || unique.len() != 30 {
        return Err(format!(
            "MLB roster directory requires 30 unique teams, received {} rows and {} unique identities",
            teams.len(),
            unique.len()
        ));
    }
    let mut succeeded = 0_i64;
    let mut failures = 0_usize;
    for (team_id, scope) in teams {
        let _ = store.mark_sync_item_attempt("mlb", "40man_team", scope, SYNC_PIPELINE_VERSION);
        match fetch(*team_id).and_then(|rows| {
            store
                .replace_mlb_roster(scope, &rows)
                .map_err(|error| error.to_string())
        }) {
            Ok(()) => {
                succeeded += 1;
                let _ =
                    store.mark_sync_item_success("mlb", "40man_team", scope, SYNC_PIPELINE_VERSION);
            }
            Err(error) => {
                failures += 1;
                let _ = store.mark_sync_item_failure(
                    "mlb",
                    "40man_team",
                    scope,
                    SYNC_PIPELINE_VERSION,
                    &error,
                );
            }
        }
    }
    if failures == 0 {
        Ok(SyncItemSuccess::complete(succeeded))
    } else if succeeded > 0 {
        Ok(SyncItemSuccess::degraded(
            succeeded,
            format!(
                "{succeeded} teams succeeded, {failures} failed; prior failed-team rosters were retained"
            ),
        ))
    } else {
        Err("all MLB 40-man roster fetches failed; prior team rosters were retained".into())
    }
}

fn team_abbreviation(team_id: i64) -> &'static str {
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

fn hitting_write(row: &crate::providers::mlb::BulkHittingSplit) -> SeasonStatWrite {
    SeasonStatWrite {
        mlbam_id: row.player.person_id,
        name: row.player.full_name.clone(),
        team_abbreviation: team_abbreviation(row.team.team_id).into(),
        stat_group: "hitting".into(),
        games: row.stat.games_played,
        plate_appearances: row.stat.plate_appearances,
        at_bats: row.stat.at_bats,
        hits: row.stat.hits,
        home_runs: row.stat.home_runs,
        runs_batted_in: row.stat.rbi,
        runs: row.stat.runs,
        stolen_bases: row.stat.stolen_bases,
        walks: row.stat.walks,
        hit_by_pitch: row.stat.hit_by_pitch,
        total_bases: row.stat.total_bases,
        strikeouts: row.stat.strikeouts,
        ..SeasonStatWrite::default()
    }
}

fn pitching_write(row: &crate::providers::mlb::BulkPitchingSplit) -> SeasonStatWrite {
    SeasonStatWrite {
        mlbam_id: row.player.person_id,
        name: row.player.full_name.clone(),
        team_abbreviation: team_abbreviation(row.team.team_id).into(),
        stat_group: "pitching".into(),
        games: row.stat.games_pitched,
        wins: row.stat.wins,
        saves: row.stat.saves,
        holds: row.stat.holds,
        strikeouts: row.stat.strikeouts,
        innings_outs: innings_outs(&row.stat.innings_pitched),
        games_started: row.stat.games_started,
        quality_starts: row.stat.quality_starts,
        hits_allowed: row.stat.hits_allowed,
        earned_runs: row.stat.earned_runs,
        pitcher_walks: row.stat.walks,
        ..SeasonStatWrite::default()
    }
}

fn merge_quality_start_writes(
    rows: &mut [SeasonStatWrite],
    counts: &std::collections::BTreeMap<i64, i64>,
) {
    for row in rows {
        if let Some(quality_starts) = counts.get(&row.mlbam_id) {
            row.quality_starts = *quality_starts;
        }
    }
}

fn innings_outs(value: &str) -> i64 {
    let (whole, partial) = value.split_once('.').unwrap_or((value, "0"));
    whole.parse::<i64>().unwrap_or(0) * 3
        + partial
            .chars()
            .next()
            .and_then(|value| value.to_digit(10))
            .map_or(0, i64::from)
            .min(2)
}

fn sync_utc_date(time: SystemTime) -> Result<String, WorkflowError> {
    let days = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WorkflowError("sync: system clock precedes the Unix epoch".into()))?
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

#[cfg(test)]
mod provider_cycle_tests {
    use super::*;
    use crate::providers::yahoo::YahooError;
    use std::cell::Cell;
    use std::io;
    use tempfile::tempdir;

    #[derive(Default)]
    struct FlushWriter {
        bytes: Vec<u8>,
        flushes: usize,
    }

    impl Write for FlushWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }

    #[test]
    fn quality_start_supplement_updates_current_pitching_writes() {
        let mut rows = vec![
            SeasonStatWrite {
                mlbam_id: 101,
                stat_group: "pitching".into(),
                quality_starts: 0,
                ..SeasonStatWrite::default()
            },
            SeasonStatWrite {
                mlbam_id: 202,
                stat_group: "pitching".into(),
                quality_starts: 4,
                ..SeasonStatWrite::default()
            },
        ];
        let counts = [(101, 12)].into_iter().collect();

        merge_quality_start_writes(&mut rows, &counts);

        assert_eq!(rows[0].quality_starts, 12);
        assert_eq!(rows[1].quality_starts, 4);
    }

    #[test]
    fn foreground_progress_is_ordered_flushed_and_not_duplicated() {
        let mut writer = FlushWriter::default();
        let mut outcomes = Vec::new();
        {
            let mut reporter = |line: &str| write_sync_progress(&mut writer, line);
            for item in ["first", "second"] {
                record_outcome(
                    &mut outcomes,
                    SyncItemOutcome {
                        source: "provider".into(),
                        item: item.into(),
                        succeeded: true,
                        skipped: false,
                        degraded: false,
                        count: 1,
                        detail: String::new(),
                    },
                    &mut reporter,
                )
                .unwrap();
            }
        }

        assert_eq!(writer.flushes, 2);
        assert_eq!(
            String::from_utf8(writer.bytes).unwrap(),
            "provider first: success (1)\nprovider second: success (1)\n"
        );
        assert_eq!(outcomes.len(), 2);
    }

    #[test]
    fn authenticated_yahoo_stops_after_three_terminal_access_failures() {
        let directory = tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let mut terminal_failures = 0;
        let mut dispatched = 0;
        let mut outcomes = Vec::new();
        for item in [
            "settings",
            "standings",
            "rosters",
            "free_agents",
            "team_identity",
        ] {
            if authenticated_yahoo_limit_reached(terminal_failures) {
                outcomes.push(skipped_sync_item(
                    &mut store,
                    "yahoo_authenticated",
                    item,
                    "league",
                ));
                continue;
            }
            dispatched += 1;
            let detail = authenticated_yahoo_error(
                "fetch authenticated Yahoo",
                YahooFantasyError::Yahoo(YahooError::Forbidden),
                &mut terminal_failures,
            );
            outcomes.push(failed_sync_item(
                &mut store,
                "yahoo_authenticated",
                item,
                "league",
                &detail,
            ));
        }

        assert_eq!(dispatched, 3);
        assert_eq!(terminal_failures, 3);
        assert!(outcomes[3..].iter().all(|outcome| outcome.skipped));
        assert!(
            outcomes[3..]
                .iter()
                .all(|outcome| outcome.line().contains("skipped"))
        );
        assert!(!authenticated_yahoo_limit_reached(0));
    }

    #[test]
    fn injected_steps_continue_after_failure_and_retain_independent_state() {
        let directory = tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let attempts = Cell::new(0);
        let mut outcomes = Vec::new();
        for (source, fails) in [
            ("yahoo", true),
            ("mlb", false),
            ("savant", false),
            ("espn", false),
        ] {
            outcomes.push(run_sync_item(
                &mut store,
                source,
                "snapshot",
                "2026",
                SyncOrigin::Manual,
                false,
                |_| {
                    attempts.set(attempts.get() + 1);
                    if fails {
                        Err("injected failure".into())
                    } else {
                        Ok(1_i64)
                    }
                },
            ));
        }
        assert_eq!(attempts.get(), 4);
        assert_eq!(
            outcomes.iter().filter(|outcome| outcome.succeeded).count(),
            3
        );
        assert_eq!(
            store
                .sync_item_state("yahoo", "snapshot", "2026")
                .unwrap()
                .unwrap()
                .error_message,
            "injected failure"
        );
        for source in ["mlb", "savant", "espn"] {
            assert_eq!(
                store
                    .sync_item_state(source, "snapshot", "2026")
                    .unwrap()
                    .unwrap()
                    .status,
                crate::store::SyncStateStatus::Complete
            );
        }
    }

    #[test]
    fn injected_all_failed_cycle_attempts_every_step() {
        let directory = tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let attempts = Cell::new(0);
        let outcomes = ["yahoo", "mlb", "savant", "espn"].map(|source| {
            run_sync_item(
                &mut store,
                source,
                "snapshot",
                "2026",
                SyncOrigin::Manual,
                false,
                |_| {
                    attempts.set(attempts.get() + 1);
                    Err::<i64, _>(format!("{source} unavailable"))
                },
            )
        });
        assert_eq!(attempts.get(), 4);
        assert!(outcomes.iter().all(|outcome| !outcome.succeeded));
    }

    #[test]
    fn automatic_freshness_skips_while_manual_refreshes() {
        let directory = tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let first = run_sync_item(
            &mut store,
            "mlb",
            "hitting",
            "2026",
            SyncOrigin::Automatic,
            false,
            |_| Ok::<i64, String>(1),
        );
        let skipped = run_sync_item(
            &mut store,
            "mlb",
            "hitting",
            "2026",
            SyncOrigin::Automatic,
            false,
            |_| -> Result<i64, String> { panic!("fresh automatic step ran") },
        );
        let manual = run_sync_item(
            &mut store,
            "mlb",
            "hitting",
            "2026",
            SyncOrigin::Manual,
            false,
            |_| Ok::<i64, String>(2),
        );
        assert!(first.succeeded && !first.skipped);
        assert!(skipped.succeeded && skipped.skipped);
        assert!(manual.succeeded && !manual.skipped);
        assert_eq!(manual.count, 2);
    }

    #[test]
    fn degraded_success_is_successful_but_visible() {
        let directory = tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let outcome = run_sync_item(
            &mut store,
            "espn",
            "odds",
            "2026-08-17",
            SyncOrigin::Manual,
            false,
            |_| {
                Ok::<SyncItemSuccess, String>(SyncItemSuccess::degraded(
                    10,
                    "2 bounded odds issues".into(),
                ))
            },
        );
        assert!(outcome.succeeded && outcome.degraded);
        assert!(outcome.line().contains("degraded"));
        let state = store
            .sync_item_state("espn", "odds", "2026-08-17")
            .unwrap()
            .unwrap();
        assert_eq!(state.status, crate::store::SyncStateStatus::Complete);
        assert_eq!(state.error_message, "2 bounded odds issues");
    }

    #[test]
    fn roster_sync_records_all_teams_and_retains_one_failed_team() {
        let directory = tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let prior = crate::store::RosterWrite {
            mlbam_id: 9000,
            name: "Prior Player".into(),
            position: "OF".into(),
            primary_type: "H".into(),
            status: "D60".into(),
            jersey_number: "9".into(),
        };
        store.replace_mlb_roster("T00", &[prior]).unwrap();
        let teams = (0..30)
            .map(|index| (index + 1, format!("T{index:02}")))
            .collect::<Vec<_>>();
        let result = sync_mlb_rosters(&mut store, &teams, |team_id| {
            if team_id == 1 {
                return Err("injected roster failure".into());
            }
            Ok(vec![crate::store::RosterWrite {
                mlbam_id: 9000 + team_id,
                name: format!("Player {team_id}"),
                position: "P".into(),
                primary_type: "P".into(),
                status: "A".into(),
                jersey_number: team_id.to_string(),
            }])
        })
        .unwrap();
        assert!(result.degraded);
        assert_eq!(result.count, 29);
        assert_eq!(store.mlb_roster("T00").unwrap()[0].name, "Prior Player");
        assert_eq!(
            store
                .sync_item_state("mlb", "40man_team", "T00")
                .unwrap()
                .unwrap()
                .error_message,
            "injected roster failure"
        );
        assert!(teams[1..].iter().all(|(_, team)| {
            store
                .sync_item_state("mlb", "40man_team", team)
                .unwrap()
                .is_some()
        }));
    }
}
