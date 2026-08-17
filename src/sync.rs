//! Foreground fantasy workflow application services.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{BufRead, Write};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config;
use crate::providers::yahoo::YahooClient;
use crate::providers::yahoo_fantasy::{YahooFantasyClient, YahooFantasySource};
use crate::store::{
    CategoryWrite, FantasySnapshotWrite, IdentityCandidate, PositionWrite, Store, StoreStatus,
    SyncMode, SyncOrigin,
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
    synchronize_with_options(league_override, false)
}

/// Synchronize manually, optionally bypassing all application freshness gates.
pub fn synchronize_with_options(
    league_override: Option<&str>,
    _force: bool,
) -> Result<String, WorkflowError> {
    // Stable Yahoo synchronization currently always acquires a complete snapshot. Keeping force
    // explicit here preserves manual execution semantics when freshness gates are introduced.
    synchronize_for_origin(league_override, SyncOrigin::Manual)
}

/// Synchronize the selected league through the shared production service.
pub fn synchronize_for_origin(
    league_override: Option<&str>,
    origin: SyncOrigin,
) -> Result<String, WorkflowError> {
    let _guard = crate::daemon::SyncGuard::acquire()
        .map_err(|error| WorkflowError::context("start synchronization", error))?;
    let mut config =
        config::read().map_err(|error| WorkflowError::context("read configuration", error))?;
    let league_key = league_override
        .filter(|key| !key.trim().is_empty())
        .unwrap_or(&config.current_league)
        .to_owned();
    if league_key.is_empty() {
        return Err(WorkflowError(
            "sync: no league selected; run b9 st -l <key> and retry".into(),
        ));
    }
    let http = Arc::new(
        HttpClient::production()
            .map_err(|error| WorkflowError::context("initialize HTTP transport", error))?,
    );
    let yahoo = Arc::new(
        YahooClient::production(http.clone())
            .map_err(|error| WorkflowError::context("initialize Yahoo", error))?,
    );
    let source = YahooFantasyClient::new(yahoo);
    let mut store =
        Store::open().map_err(|error| WorkflowError::context("open database", error))?;
    let mlb = crate::providers::mlb::MlbClient::production(http.clone());
    let savant = crate::providers::savant::SavantClient::production(http);
    let mut identities = |season: i32| {
        let mut values = Vec::new();
        if let Ok(rows) = mlb.fetch_bulk_hitting_stats(i64::from(season), "R") {
            values.extend(rows.into_iter().map(|row| IdentityCandidate {
                mlbam_id: row.player.person_id,
                name: row.player.full_name,
                team: team_abbreviation(row.team.team_id).to_owned(),
                role: "B".into(),
            }));
        }
        if let Ok(rows) = mlb.fetch_bulk_pitching_stats(i64::from(season), "R") {
            values.extend(rows.into_iter().map(|row| IdentityCandidate {
                mlbam_id: row.player.person_id,
                name: row.player.full_name,
                team: team_abbreviation(row.team.team_id).to_owned(),
                role: "P".into(),
            }));
        }
        values
    };
    let summary =
        synchronize_with_origin(&source, &mut store, &league_key, origin, &mut identities)?;
    let season = store
        .fantasy_season(&league_key)
        .map_err(|error| WorkflowError::context("read Statcast season", error))?
        .ok_or_else(|| {
            WorkflowError(
                "sync Statcast: league season is unavailable; retry after Yahoo synchronization"
                    .into(),
            )
        })?;
    let batting = savant
        .fetch_batting(season)
        .map_err(|error| WorkflowError::context("fetch Statcast batting snapshot", error))?;
    let pitching = savant
        .fetch_pitching(season)
        .map_err(|error| WorkflowError::context("fetch Statcast pitching snapshot", error))?;
    store
        .replace_statcast_snapshot(season, "batting", &batting)
        .map_err(|error| WorkflowError::context("persist Statcast batting snapshot", error))?;
    store
        .replace_statcast_snapshot(season, "pitching", &pitching)
        .map_err(|error| WorkflowError::context("persist Statcast pitching snapshot", error))?;
    if league_override.is_none() || config.current_league == league_key {
        config.current_team_key = summary.team_key;
        config::write(&config)
            .map_err(|error| WorkflowError::context("save team identity", error))?;
    }
    Ok(format!(
        "Synced {} teams, {} players, {} roster slots, and {} MLB identities.\n",
        summary.teams, summary.players, summary.roster_slots, summary.mlb_identities
    ))
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
