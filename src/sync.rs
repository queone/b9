//! Foreground fantasy workflow application services.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use crate::config;
use crate::providers::yahoo::YahooClient;
use crate::providers::yahoo_fantasy::{YahooFantasyClient, YahooFantasySource};
use crate::store::{
    CategoryWrite, FantasySnapshotWrite, IdentityCandidate, PositionWrite, Store, SyncMode,
    SyncOrigin,
};
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

/// Render production status and optionally persist a validated league selection.
pub fn status(requested_league: Option<&str>) -> Result<String, WorkflowError> {
    let yahoo = production_yahoo()?;
    let source = YahooFantasyClient::new(yahoo.clone());
    let token = yahoo
        .token_status()
        .map_err(|error| WorkflowError::context("read Yahoo status", error))?;
    let mut config =
        config::read().map_err(|error| WorkflowError::context("read configuration", error))?;
    let leagues = if token.valid || token.has_refresh {
        source
            .user_leagues()
            .map_err(|error| WorkflowError::context("discover leagues", error))?
    } else {
        Vec::new()
    };
    let mut input = std::io::BufReader::new(std::io::stdin());
    let mut prompt = Vec::new();
    let selected = select_league(
        &leagues,
        requested_league,
        std::io::stdin().is_terminal(),
        &mut input,
        &mut prompt,
    )?;
    if let Some(selected) = selected
        && config.current_league != selected
    {
        config.current_league = selected;
        config.current_team_key.clear();
        config::write(&config)
            .map_err(|error| WorkflowError::context("save league selection", error))?;
    }
    let database = crate::store::database_path()
        .map_err(|error| WorkflowError::context("resolve database status", error))?;
    let database_state = if Path::new(&database).is_file() {
        "present"
    } else {
        "absent"
    };
    let store_status = crate::store::inspect_status_at(&database, &config.current_league)
        .map_err(|error| WorkflowError::context("inspect database status", error))?;
    let mut output = String::from_utf8(prompt)
        .map_err(|error| WorkflowError::context("render league selection", error))?;
    output.push_str(&format!(
        "Yahoo: {}\nDatabase: {}\nSelected league: {}\n",
        if token.valid {
            "authenticated"
        } else if token.has_refresh {
            "refresh required"
        } else {
            "not authenticated"
        },
        database_state,
        if config.current_league.is_empty() {
            "none"
        } else {
            &config.current_league
        }
    ));
    output.push_str(&format!(
        "League freshness: {}\nLatest sync: {}\n",
        store_status
            .league_synced_at
            .map_or_else(|| "none".into(), |value| format!("unix {value}")),
        store_status.latest_sync_status.map_or_else(
            || "none".into(),
            |status| format!(
                "{} at unix {}",
                status,
                store_status.latest_sync_at.unwrap_or_default()
            )
        )
    ));
    if !leagues.is_empty() {
        output.push_str("Leagues:\n");
        for league in leagues {
            output.push_str(&format!("  {}  {}\n", league.league_key, league.name));
        }
    }
    Ok(output)
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
    let run = store
        .start_sync_run(SyncMode::Live, SyncOrigin::Manual)
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
        Ok(summary)
    })();
    if result.is_err() {
        let _ = store.fail_sync_run(run);
    }
    result
}

/// Synchronize the selected league's stable normalized Yahoo data in the foreground.
pub fn synchronize(league_override: Option<&str>) -> Result<String, WorkflowError> {
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
    let mlb = crate::providers::mlb::MlbClient::production(http);
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
    let summary = synchronize_with(&source, &mut store, &league_key, &mut identities)?;
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
