//! Foreground fantasy workflow application services.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config;
use crate::domain::FantasyTeam;
use crate::providers::yahoo_fantasy::YahooFantasySource;
use crate::providers::yahoo_public::YahooPublicClient;
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

/// Render the settled, fixed-order `b9 st` dashboard.
///
/// Field order is contracted: last run and completion state, database
/// path/size/schema, MLB/Yahoo identity counts, provider freshness, circuit
/// state and bounded last error, unmatched-player count, then selected
/// league/config paths.
pub fn render_dashboard(
    database_path: &Path,
    config_path: &Path,
    config: &config::Config,
    status: &StoreStatus,
    _now: i64,
    mode: HelpColorMode,
) -> String {
    let has_snapshot = status.mlb_identity_count > 0 || status.unmatched_player_count > 0;
    let legacy_authenticated_yahoo_failure = status
        .provider_last_error
        .as_deref()
        .is_some_and(is_legacy_authenticated_yahoo_error);
    let last_run_status = if legacy_authenticated_yahoo_failure {
        None
    } else {
        status.last_run_status.as_deref()
    };
    let last_run_at = if legacy_authenticated_yahoo_failure {
        None
    } else {
        status.last_run_at
    };
    let last_run = match (last_run_status, last_run_at) {
        (Some(run_status), Some(at)) => format!("{run_status} at unix {at}"),
        _ => "none".to_owned(),
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
        "unavailable".to_owned()
    };

    let provider_freshness = if !has_snapshot {
        "unavailable".to_owned()
    } else {
        status
            .provider_freshness_at
            .map_or_else(|| "none".to_owned(), |at| format!("unix {at}"))
    };

    let circuit_open = status.circuit_open && !legacy_authenticated_yahoo_failure;
    let provider_failure_count = if legacy_authenticated_yahoo_failure {
        0
    } else {
        status.provider_failure_count
    };
    let provider_failures = format!(
        "{} ({})",
        if circuit_open { "blocked" } else { "ready" },
        provider_failure_count
    );
    let last_error = if legacy_authenticated_yahoo_failure {
        "none"
    } else {
        status.provider_last_error.as_deref().unwrap_or("none")
    };

    let unmatched = if has_snapshot {
        status.unmatched_player_count.to_string()
    } else {
        "unavailable".to_owned()
    };

    let selected_league = if config.current_league.is_empty() {
        terminal::injury_status("none", mode)
    } else {
        terminal::dim(&config.current_league, mode)
    };

    let mut output = String::new();
    for (label, value) in [
        ("Yahoo", "public endpoints".to_owned()),
        ("Last run", last_run),
        ("Database", database),
        ("Identities", identities),
        ("Provider freshness", provider_freshness),
        (
            "FanGraphs",
            status
                .fangraphs_sync
                .clone()
                .unwrap_or_else(|| "none".into()),
        ),
        (
            "FantasyPros",
            status
                .fantasypros_sync
                .clone()
                .unwrap_or_else(|| "none".into()),
        ),
        ("Provider failures", provider_failures),
        ("Last provider error", last_error.to_owned()),
        ("Unmatched players", unmatched),
        ("League", selected_league),
        (
            "Config",
            terminal::dim(&config_path.display().to_string(), mode),
        ),
    ] {
        let value = if label == "League" || label == "Config" {
            value
        } else {
            terminal::dim(&value, mode)
        };
        output.push_str(&format!("{label}: {value}\n"));
    }
    if !has_snapshot {
        output.push_str("No local snapshot; run b9 sync.\n");
    }
    output
}

fn is_legacy_authenticated_yahoo_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("fetch authenticated yahoo")
        || error.contains("run b9 login")
        || error.contains("reauthorize")
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
        assert!(output.contains("Yahoo: public endpoints"));
        assert!(output.contains("Last run: none"));
        assert!(!output.contains("Service:"));
        assert!(!output.contains("Next run:"));
        assert!(output.contains("Database: /absent/b9.db (absent, schema unknown)"));
        assert!(output.contains("Identities: unavailable"));
        assert!(output.contains("Provider freshness: unavailable"));
        assert!(output.contains("Provider failures: ready (0)"));
        assert!(output.contains("Unmatched players: unavailable"));
        assert!(output.contains("League: none"));
        assert!(output.contains("Config: /absent/config.json"));
        assert!(output.contains("No local snapshot; run b9 sync."));
        assert!(!output.contains("0 MLB"));
        assert!(!output.contains("Unmatched players: 0"));

        let colored = render_dashboard(
            Path::new("/absent/b9.db"),
            Path::new("/absent/config.json"),
            &Config::default(),
            &StoreStatus::default(),
            0,
            HelpColorMode::Color,
        );
        assert!(colored.contains("League: \u{1b}[38;5;196mnone\u{1b}[0m"));
    }

    #[test]
    fn populated_snapshot_reports_real_counts_without_the_no_snapshot_hint() {
        let status = StoreStatus {
            mlb_identity_count: 512,
            yahoo_identity_count: 480,
            unmatched_player_count: 6,
            provider_freshness_at: Some(100),
            last_run_status: Some("success".into()),
            last_run_at: Some(100),
            database_bytes: Some(1024),
            schema_version: Some(3),
            fangraphs_sync: Some("failed: truncated".into()),
            fantasypros_sync: Some("complete at unix 90".into()),
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
        assert!(output.contains("Last run: success at unix 100"));
        assert!(!output.contains("Service:"));
        assert!(!output.contains("Next run:"));
        assert!(output.contains("Database: /srv/b9/.config/b9/b9.db (1024 bytes, schema v3)"));
        assert!(output.contains("Identities: 512 MLB, 480 Yahoo"));
        assert!(output.contains("Provider freshness: unix 100"));
        assert!(output.contains("FanGraphs: failed: truncated"));
        assert!(output.contains("FantasyPros: complete at unix 90"));
        assert!(output.contains("Unmatched players: 6"));
        assert!(output.contains("League: 431.l.12345"));
        assert!(!output.contains("No local snapshot"));
    }

    #[test]
    fn legacy_authenticated_yahoo_failure_is_not_rendered_as_current_state() {
        let status = StoreStatus {
            mlb_identity_count: 8_835,
            yahoo_identity_count: 2_507,
            circuit_open: true,
            provider_failure_count: 5,
            provider_last_error: Some(
                "fetch authenticated Yahoo settings: Yahoo API returned HTTP 403; run b9 login to reauthorize"
                    .into(),
            ),
            last_run_status: Some("failed".into()),
            last_run_at: Some(1_787_067_588),
            ..StoreStatus::default()
        };
        let output = render_dashboard(
            Path::new("/db"),
            Path::new("/config.json"),
            &Config::default(),
            &status,
            0,
            HelpColorMode::Plain,
        );
        assert!(output.contains("Yahoo: public endpoints"));
        assert!(output.contains("Last run: none"));
        assert!(output.contains("Provider failures: ready (0)"));
        assert!(output.contains("Last provider error: none"));
        assert!(!output.contains("login"));
        assert!(!output.contains("reauthorize"));
    }
}

/// Choose the primary team through deterministic matching or an injectable prompt.
pub fn select_primary_team(
    teams: &[FantasyTeam],
    requested: Option<&str>,
    interactive: bool,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<String, WorkflowError> {
    if let Some(query) = requested.filter(|value| !value.trim().is_empty()) {
        let query = query.trim();
        if let Some(team) = teams.iter().find(|team| team.team_key == query) {
            return Ok(team.team_key.clone());
        }
        let exact = teams
            .iter()
            .filter(|team| team.name.eq_ignore_ascii_case(query))
            .collect::<Vec<_>>();
        if let [team] = exact.as_slice() {
            return Ok(team.team_key.clone());
        }
        let lowered = query.to_lowercase();
        let partial = teams
            .iter()
            .filter(|team| team.name.to_lowercase().contains(&lowered))
            .collect::<Vec<_>>();
        if let [team] = partial.as_slice() {
            return Ok(team.team_key.clone());
        }
        if !interactive {
            return if partial.is_empty() {
                Err(WorkflowError(format!(
                    "select primary team: no team matches {query:?}; run b9 sync -T <key-or-name> and retry"
                )))
            } else {
                Err(WorkflowError(format!(
                    "select primary team: {query:?} is ambiguous; use an exact team key or name"
                )))
            };
        }
    }
    match teams {
        [] => Err(WorkflowError(
            "select primary team: the public league contains no teams; verify the league id and retry"
                .into(),
        )),
        [team] => Ok(team.team_key.clone()),
        _ if !interactive => Err(WorkflowError(
            "select primary team: run b9 sync -T <key-or-name> and retry".into(),
        )),
        _ => {
            writeln!(output, "Select your primary team:")
                .map_err(|error| WorkflowError::context("write team selection", error))?;
            for (index, team) in teams.iter().enumerate() {
                writeln!(output, "  {}. {}  {}", index + 1, team.team_key, team.name)
                    .map_err(|error| WorkflowError::context("write team selection", error))?;
            }
            write!(output, "Choice: ")
                .map_err(|error| WorkflowError::context("write team selection", error))?;
            let mut choice = String::new();
            input
                .read_line(&mut choice)
                .map_err(|error| WorkflowError::context("read team selection", error))?;
            let index = choice
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| {
                    WorkflowError(
                        "select primary team: enter one of the displayed numbers and retry".into(),
                    )
                })?;
            teams
                .get(index - 1)
                .map(|team| team.team_key.clone())
                .ok_or_else(|| {
                    WorkflowError(
                        "select primary team: enter one of the displayed numbers and retry".into(),
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

/// A held cross-process synchronization boundary.
struct SyncGuard {
    _claim: File,
}

impl SyncGuard {
    fn acquire() -> Result<Self, WorkflowError> {
        let directory = sync_runtime_directory()?;
        Self::acquire_at(&directory)
    }

    fn acquire_at(directory: &Path) -> Result<Self, WorkflowError> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(directory)
            .map_err(|error| WorkflowError::context("create synchronization runtime", error))?;
        let path = directory.join("sync.lock");
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let claim = options
            .open(&path)
            .map_err(|error| WorkflowError::context("open synchronization lock", error))?;
        claim.try_lock().map_err(|_| {
            WorkflowError::context(
                "start synchronization",
                "another synchronization is running",
            )
        })?;
        Ok(Self { _claim: claim })
    }
}

fn sync_runtime_directory() -> Result<PathBuf, WorkflowError> {
    let config = config::config_path()
        .map_err(|error| WorkflowError::context("resolve synchronization runtime", error))?;
    config
        .parent()
        .map(|parent| parent.join("runtime"))
        .ok_or_else(|| {
            WorkflowError::context(
                "resolve synchronization runtime",
                "configuration has no parent",
            )
        })
}

/// Synchronize through injected provider, store, and optional identity boundaries.
pub fn synchronize_with(
    source: &dyn YahooFantasySource,
    store: &mut Store,
    league_key: &str,
    selected_team_key: &str,
    identities_for_season: &mut dyn FnMut(i32) -> Vec<IdentityCandidate>,
) -> Result<SyncSummary, WorkflowError> {
    synchronize_with_origin(
        source,
        store,
        league_key,
        selected_team_key,
        SyncOrigin::Manual,
        identities_for_season,
    )
}

/// Synchronize through injected boundaries with an explicit durable caller origin.
pub fn synchronize_with_origin(
    source: &dyn YahooFantasySource,
    store: &mut Store,
    league_key: &str,
    selected_team_key: &str,
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
        if !teams.iter().any(|team| team.team_key == selected_team_key) {
            return Err(WorkflowError(
                "sync: selected primary team is outside the complete standings; choose it again and retry"
                    .into(),
            ));
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
            team_key: selected_team_key.to_owned(),
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
    synchronize_with_options(league_override, None)
}

/// Synchronize manually with an optional primary-team override.
pub fn synchronize_with_options(
    league_override: Option<&str>,
    team_override: Option<&str>,
) -> Result<String, WorkflowError> {
    synchronize_for_origin_reporting(
        league_override,
        SyncOrigin::Manual,
        &mut |_| Ok(()),
        true,
        team_override,
    )
}

/// Synchronize manually while writing and flushing each completed step immediately.
pub fn synchronize_with_options_streaming(
    league_override: Option<&str>,
    team_override: Option<&str>,
    output: &mut dyn Write,
) -> Result<String, WorkflowError> {
    let mut reporter = |line: &str| write_sync_progress(output, line);
    synchronize_for_origin_reporting(
        league_override,
        SyncOrigin::Manual,
        &mut reporter,
        false,
        team_override,
    )
}

fn synchronize_for_origin_reporting(
    league_override: Option<&str>,
    origin: SyncOrigin,
    reporter: &mut dyn FnMut(&str) -> Result<(), WorkflowError>,
    include_step_lines: bool,
    team_override: Option<&str>,
) -> Result<String, WorkflowError> {
    let _guard = SyncGuard::acquire()?;
    let mut config =
        config::read().map_err(|error| WorkflowError::context("read configuration", error))?;
    let original_config = config.clone();
    let mut requested_league = league_override
        .filter(|key| !key.trim().is_empty())
        .unwrap_or(&config.current_league)
        .to_owned();
    if requested_league.is_empty() && !config.pull_public_league_id.is_empty() {
        requested_league = config.pull_public_league_id.clone();
    }
    if requested_league.is_empty() {
        if !std::io::stdin().is_terminal() {
            return Err(WorkflowError(
                "sync: no Yahoo league configured; run b9 sync -l <league-id-or-key> -T <team> and retry"
                    .into(),
            ));
        }
        let mut prompt = std::io::stderr();
        write!(prompt, "Yahoo league id or key: ")
            .and_then(|()| prompt.flush())
            .map_err(|error| WorkflowError::context("write league prompt", error))?;
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|error| WorkflowError::context("read league prompt", error))?;
        requested_league = line.trim().to_owned();
    }
    let league_key = crate::providers::yahoo_public::canonical_public_league_key(&requested_league)
        .map_err(|error| WorkflowError::context("resolve public Yahoo league", error))?;
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
        "fantasy",
        public_scope,
        origin,
        force,
        |store| {
            let source = YahooPublicClient::shared(http.clone());
            let settings = source
                .league_settings(&league_key)
                .map_err(|error| format!("fetch public Yahoo settings: {error}"))?;
            let teams = source
                .standings(&league_key)
                .map_err(|error| format!("fetch public Yahoo standings: {error}"))?;
            let rosters = source
                .league_rosters(&league_key)
                .map_err(|error| format!("fetch public Yahoo rosters: {error}"))?;
            let free_agents = source
                .free_agents(&league_key)
                .map_err(|error| format!("fetch public Yahoo free agents: {error}"))?;
            let requested_team = team_override
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    (!config.current_team_key.is_empty())
                        .then_some(config.current_team_key.as_str())
                });
            let interactive = std::io::stdin().is_terminal();
            let mut input = std::io::BufReader::new(std::io::stdin());
            let mut prompt = std::io::stderr();
            let team_key =
                select_primary_team(&teams, requested_team, interactive, &mut input, &mut prompt)
                    .map_err(|error| error.to_string())?;
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
            let count = snapshot.players.len() as i64;
            store
                .replace_fantasy_snapshot(&snapshot)
                .map_err(|error| {
                    format!(
                        "persist public Yahoo fantasy snapshot: {error}; prior complete data was retained"
                    )
                })?;
            config.current_league = league_key.clone();
            config.current_team_key = team_key;
            config.pull_public_league_id.clear();
            Ok(count)
        },
    );
    record_outcome(&mut outcomes, public, reporter)?;

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

    record_outcome(
        &mut outcomes,
        run_sync_item(
            &mut store,
            "fangraphs",
            "snapshot",
            &season.to_string(),
            origin,
            force,
            |store| {
                use crate::providers::fangraphs::{
                    FangraphsClient, LeaderRow, ProjectionRow as FgProjection,
                };
                let client = FangraphsClient::new(http.clone());
                let leaders:Vec<LeaderRow>=client.fetch_json(&format!("https://www.fangraphs.com/api/leaders/major-league/data?pos=all&stats=bat&lg=all&qual=0&season={season}&season1={season}&type=8&month=0&pageItems=2000&ind=0")).map_err(|e|format!("fetch FanGraphs leaderboard: {e}; prior data was retained"))?;
                if leaders.len() < 100 {
                    return Err("validate FanGraphs leaderboard: fewer than 100 rows; prior data was retained".into());
                }
                let crosswalk = leaders
                    .iter()
                    .filter_map(|r| r.mlbam_id.map(|id| (r.fangraphs_id, id)))
                    .collect::<BTreeMap<_, _>>();
                let batted = leaders
                    .iter()
                    .filter_map(|r| {
                        r.mlbam_id.map(|id| crate::store::FangraphsBattedBallWrite {
                            mlbam_id: id,
                            season,
                            fb_pct: r.fb_pct,
                            hr_fb_pct: r.hr_fb_pct,
                        })
                    })
                    .collect::<Vec<_>>();
                let systems = [("steamer", 0.40), ("zips", 0.35), ("atc", 0.25)];
                let mut raw = Vec::new();
                for (system, _) in systems {
                    for group in ["bat", "pit"] {
                        let rows:Vec<FgProjection>=client.fetch_json(&format!("https://www.fangraphs.com/api/projections?type={system}&stats={group}&pos=all&season={season}&sortstat=ADP&sortorder=desc&page=1_5000")).map_err(|e|format!("fetch FanGraphs {system} projections: {e}; prior data was retained"))?;
                        for r in rows {
                            if let Some(id) = crosswalk.get(&r.fangraphs_id) {
                                raw.push(crate::store::ProjectionWrite {
                                    mlbam_id: *id,
                                    season,
                                    source: system.into(),
                                    stat_group: if group == "bat" {
                                        "batting"
                                    } else {
                                        "pitching"
                                    }
                                    .into(),
                                    pa: r.pa,
                                    ip: r.ip,
                                    hr: r.hr,
                                    r: r.r,
                                    rbi: r.rbi,
                                    sb: r.sb,
                                    avg: r.avg,
                                    obp: r.obp,
                                    slg: r.slg,
                                    era: r.era,
                                    whip: r.whip,
                                    k: r.k,
                                    w: r.w,
                                    sv: r.sv,
                                    bb: r.bb,
                                });
                            }
                        }
                    }
                }
                if raw.len() < 100 {
                    return Err("validate FanGraphs projections: fewer than 100 resolved rows; prior data was retained".into());
                }
                let mut grouped: BTreeMap<(i64, String), Vec<crate::store::ProjectionWrite>> =
                    BTreeMap::new();
                for row in &raw {
                    grouped
                        .entry((row.mlbam_id, row.stat_group.clone()))
                        .or_default()
                        .push(row.clone());
                }
                for ((id, group), rows) in grouped {
                    let total = rows
                        .iter()
                        .map(|r| {
                            systems
                                .iter()
                                .find(|(s, _)| *s == r.source)
                                .map_or(0.0, |(_, w)| *w)
                        })
                        .sum::<f64>();
                    if total > 0.0 {
                        let mut blend = crate::store::ProjectionWrite {
                            mlbam_id: id,
                            season,
                            source: "blend".into(),
                            stat_group: group,
                            ..Default::default()
                        };
                        for r in rows {
                            let w = systems
                                .iter()
                                .find(|(s, _)| *s == r.source)
                                .map_or(0.0, |(_, w)| *w)
                                / total;
                            blend.pa += r.pa * w;
                            blend.ip += r.ip * w;
                            blend.hr += r.hr * w;
                            blend.r += r.r * w;
                            blend.rbi += r.rbi * w;
                            blend.sb += r.sb * w;
                            blend.avg += r.avg * w;
                            blend.obp += r.obp * w;
                            blend.slg += r.slg * w;
                            blend.era += r.era * w;
                            blend.whip += r.whip * w;
                            blend.k += r.k * w;
                            blend.w += r.w * w;
                            blend.sv += r.sv * w;
                            blend.bb += r.bb * w;
                        }
                        raw.push(blend)
                    }
                }
                let chart = crate::providers::fangraphs_closer_chart::fetch(http.clone())
                    .map_err(|e| e.to_string())?;
                if chart
                    .iter()
                    .map(|row| &row.team)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    < 30
                {
                    return Err("validate FanGraphs closer chart: fewer than 30 teams; prior data was retained".into());
                }
                let closers = chart
                    .into_iter()
                    .filter(|r| {
                        matches!(r.role.as_str(), "Closer" | "Co-Closer" | "Closer Committee")
                    })
                    .map(|r| (r.team, r.name))
                    .collect::<Vec<_>>();
                let count = store
                    .replace_fangraphs_snapshot(season, &raw, &batted, &closers)
                    .map_err(|e| e.to_string())?;
                Ok(count as i64)
            },
        ),
        reporter,
    )?;

    record_outcome(
        &mut outcomes,
        run_sync_item(
            &mut store,
            "fantasypros",
            "ecr",
            &season.to_string(),
            origin,
            force,
            |store| {
                let rows = crate::providers::fantasypros::fetch(http.clone())
                    .map_err(|e| format!("fetch FantasyPros ECR: {e}; prior data was retained"))?;
                if rows.len() < 100 {
                    return Err(
                        "validate FantasyPros ECR: fewer than 100 rows; prior data was retained"
                            .into(),
                    );
                }
                let writes = rows
                    .into_iter()
                    .map(|r| (r.yahoo_player_id, r.name, r.team, r.rank))
                    .collect::<Vec<_>>();
                store
                    .replace_ecr(&writes)
                    .map(|n| n as i64)
                    .map_err(|e| e.to_string())
            },
        ),
        reporter,
    )?;

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
            "sync failed: every provider failed{detail}\nCheck network access and provider availability, then retry"
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
    use std::cell::Cell;
    use std::io;
    use tempfile::tempdir;

    #[test]
    fn foreground_sync_lock_is_exclusive_and_persistent() {
        let directory = tempdir().unwrap();
        let first = SyncGuard::acquire_at(directory.path()).unwrap();
        assert!(SyncGuard::acquire_at(directory.path()).is_err());
        drop(first);
        assert!(directory.path().join("sync.lock").exists());
        assert!(SyncGuard::acquire_at(directory.path()).is_ok());
    }

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
