//! `pp` (alias `pull-public`) application workflow: fetch Yahoo's
//! unauthenticated public redzone feed and write it through the same
//! durable-store boundaries `sync` uses. Kept independent from `src/sync.rs`
//! — this is a distinct, unauthenticated trust boundary, not a variant of
//! OAuth `sync`, and it never touches the OAuth circuit breaker.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{BufRead, IsTerminal, Write};

use crate::config::{self, Config};
use crate::providers::yahoo_public::{YahooPublicClient, league_id_from_key};
use crate::store::{FantasySnapshotWrite, PositionWrite, Store, SyncMode, SyncOrigin};

/// One `pp` workflow failure.
#[derive(Debug)]
pub struct PublicPullError(String);

impl PublicPullError {
    fn context(operation: &str, error: impl fmt::Display) -> Self {
        Self(format!("{operation}: {error}"))
    }
}

impl fmt::Display for PublicPullError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PublicPullError {}

/// Fetch and persist the operator's configured public league data.
///
/// Owns the *only* production `config::read`/`config::write` calls in this
/// module — `pull_with` and everything it calls operate on an in-memory
/// `Config` and never touch the filesystem for it, so they're safe to unit
/// test without redirecting `HOME`.
pub fn pull(requested_league: Option<&str>) -> Result<String, PublicPullError> {
    let client = YahooPublicClient::production()
        .map_err(|error| PublicPullError::context("initialize public feed client", error))?;
    let mut store =
        Store::open().map_err(|error| PublicPullError::context("open database", error))?;
    let mut config =
        config::read().map_err(|error| PublicPullError::context("read configuration", error))?;
    let before = config.clone();
    let mut input = std::io::BufReader::new(std::io::stdin());
    let mut output = std::io::stdout();
    let result = pull_with(
        &client,
        &mut store,
        &mut config,
        requested_league,
        std::io::stdin().is_terminal(),
        &mut input,
        &mut output,
    );
    if config != before {
        config::write(&config)
            .map_err(|error| PublicPullError::context("save league selection", error))?;
    }
    result
}

/// Injectable-boundary `pp` workflow shared by production and tests.
pub fn pull_with(
    client: &YahooPublicClient,
    store: &mut Store,
    config: &mut Config,
    requested_league: Option<&str>,
    interactive: bool,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<String, PublicPullError> {
    let (league_id, league_key) =
        resolve_league(config, requested_league, interactive, input, output)?;
    let run = store
        .start_sync_run(SyncMode::Live, SyncOrigin::PublicPull)
        .map_err(|error| PublicPullError::context("start public pull run", error))?;
    let result = (|| -> Result<FantasySnapshotWrite, PublicPullError> {
        let mut feed = client
            .fetch_redzone(&league_id, &league_key)
            .map_err(|error| PublicPullError::context("fetch public feed", error))?;
        client
            .enrich_player_ranks(&mut feed.players)
            .map_err(|error| PublicPullError::context("fetch public player ranks", error))?;
        let snapshot = public_snapshot(feed);
        store
            .replace_fantasy_snapshot(&snapshot)
            .map_err(|error| PublicPullError::context("persist public snapshot", error))?;
        Ok(snapshot)
    })();
    match &result {
        Ok(snapshot) => {
            let mut counts = BTreeMap::new();
            counts.insert("teams".into(), snapshot.teams.len() as i64);
            counts.insert("players".into(), snapshot.players.len() as i64);
            counts.insert("roster_slots".into(), snapshot.slots.len() as i64);
            store
                .complete_sync_run(run, &counts)
                .map_err(|error| PublicPullError::context("complete public pull run", error))?;
        }
        Err(_) => {
            let _ = store.fail_sync_run(run);
        }
    }
    let snapshot = result?;
    Ok(format!(
        "Fetched {} teams, {} players, {} roster slots from Yahoo's public feed (league {league_id}).\n",
        snapshot.teams.len(),
        snapshot.players.len(),
        snapshot.slots.len(),
    ))
}

/// Normalize a fetched public feed through the same snapshot shape used by `pp`.
pub(crate) fn public_snapshot(
    feed: crate::providers::yahoo_public::RedzoneFeed,
) -> FantasySnapshotWrite {
    FantasySnapshotWrite {
        league: feed.league,
        current_week: Some(feed.week),
        categories: Vec::new(),
        positions: feed
            .roster_positions
            .into_iter()
            .map(|row| PositionWrite {
                position: row.position.to_string(),
                count: row.count,
            })
            .collect(),
        teams: feed.teams,
        players: feed.players,
        slots: feed.slots,
    }
}

/// Resolve the numeric league id and the b9 storage key to write it under.
///
/// Id order: an explicit override; the league already selected via OAuth
/// `login`/`sync`; a previously saved `pp`-only selection; only then an
/// interactive prompt, whose answer is saved for next time.
///
/// Storage key: reuse `config.current_league` verbatim when it already
/// resolves to the same numeric league (so `sync`-written and `pp`-written
/// data land in the same place and existing commands find either). A raw
/// full key passed via `-l/--league` is likewise trusted as-is. Otherwise
/// synthesize `public.{league_id}` — there is no real Yahoo key to reuse —
/// and set `config.current_league` to it so `r`/`m`/`h`/`p`/etc. can find
/// what was just fetched; clear `current_team_key`, since it named a team in
/// whatever league was previously selected.
fn resolve_league(
    config: &mut Config,
    requested: Option<&str>,
    interactive: bool,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<(String, String), PublicPullError> {
    if let Some(value) = requested {
        let league_id = league_id_from_key(value)
            .map_err(|error| PublicPullError::context("resolve requested league", error))?;
        let league_key = if value.trim().contains(".l.") {
            value.trim().to_owned()
        } else {
            storage_key_for(config, &league_id)
        };
        apply_resolution(config, &league_id, &league_key);
        return Ok((league_id, league_key));
    }
    if !config.current_league.is_empty()
        && let Ok(league_id) = league_id_from_key(&config.current_league)
    {
        // Keep the fallback in sync so it reflects the league `pp` is
        // actually resolving to, in case `current_league` is later cleared.
        config.pull_public_league_id = league_id.clone();
        return Ok((league_id, config.current_league.clone()));
    }
    if !config.pull_public_league_id.is_empty() {
        let league_id = config.pull_public_league_id.clone();
        let league_key = storage_key_for(config, &league_id);
        apply_resolution(config, &league_id, &league_key);
        return Ok((league_id, league_key));
    }
    if !interactive {
        return Err(PublicPullError(
            "pp: no league configured; run b9 login and b9 sync first, or run b9 pp -l <league id> and retry".into(),
        ));
    }
    writeln!(
        output,
        "No configured league found. Enter your Yahoo league id (the number in your league's URL):"
    )
    .map_err(|error| PublicPullError::context("write league prompt", error))?;
    write!(output, "League id: ")
        .map_err(|error| PublicPullError::context("write league prompt", error))?;
    output
        .flush()
        .map_err(|error| PublicPullError::context("flush league prompt", error))?;
    let mut line = String::new();
    input
        .read_line(&mut line)
        .map_err(|error| PublicPullError::context("read league id", error))?;
    let league_id = league_id_from_key(line.trim())
        .map_err(|error| PublicPullError::context("resolve entered league", error))?;
    let league_key = storage_key_for(config, &league_id);
    apply_resolution(config, &league_id, &league_key);
    Ok((league_id, league_key))
}

/// Reuse the existing OAuth-derived key when it names the same league;
/// otherwise synthesize a `pp`-only key.
fn storage_key_for(config: &Config, league_id: &str) -> String {
    if !config.current_league.is_empty()
        && league_id_from_key(&config.current_league).ok().as_deref() == Some(league_id)
    {
        return config.current_league.clone();
    }
    format!("public.{league_id}")
}

fn apply_resolution(config: &mut Config, league_id: &str, league_key: &str) {
    config.pull_public_league_id = league_id.to_owned();
    if config.current_league != league_key {
        config.current_league = league_key.to_owned();
        config.current_team_key.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn resolve_prefers_override_then_current_league_then_saved_then_prompt() {
        let mut output = Vec::new();

        // A full-key override is trusted as-is and sets current_league so
        // existing commands find the data; current_team_key is cleared.
        let mut config = Config {
            current_team_key: "469.l.1.t.9".into(),
            ..Config::default()
        };
        let (league_id, league_key) = resolve_league(
            &mut config,
            Some("469.l.170874"),
            false,
            &mut Cursor::new(""),
            &mut output,
        )
        .unwrap();
        assert_eq!(league_id, "170874");
        assert_eq!(league_key, "469.l.170874");
        assert_eq!(config.pull_public_league_id, "170874");
        assert_eq!(config.current_league, "469.l.170874");
        assert!(config.current_team_key.is_empty());

        // Existing OAuth-selected league wins over a saved pp-only value, no
        // prompt, and its real key is reused verbatim for storage.
        let mut config = Config {
            current_league: "469.l.555".into(),
            pull_public_league_id: "999".into(),
            ..Config::default()
        };
        let (league_id, league_key) =
            resolve_league(&mut config, None, false, &mut Cursor::new(""), &mut output).unwrap();
        assert_eq!(league_id, "555");
        assert_eq!(
            league_key, "469.l.555",
            "must reuse the real key, not synthesize one"
        );
        assert_eq!(
            config.pull_public_league_id, "555",
            "the fallback must stay in sync even when current_league resolves"
        );

        // Saved pp-only value is used without prompting when current_league
        // is empty, and current_league gets set to the synthetic key so
        // downstream commands can find the fetched data.
        let mut config = Config {
            pull_public_league_id: "42".into(),
            ..Config::default()
        };
        let (league_id, league_key) =
            resolve_league(&mut config, None, false, &mut Cursor::new(""), &mut output).unwrap();
        assert_eq!(league_id, "42");
        assert_eq!(league_key, "public.42");
        assert_eq!(config.current_league, "public.42");
        assert!(output.is_empty(), "must not prompt when a value is known");
    }

    #[test]
    fn resolve_reuses_the_real_key_when_a_bare_number_override_matches_it() {
        let mut config = Config {
            current_league: "469.l.170874".into(),
            ..Config::default()
        };
        let mut output = Vec::new();
        let (league_id, league_key) = resolve_league(
            &mut config,
            Some("170874"),
            false,
            &mut Cursor::new(""),
            &mut output,
        )
        .unwrap();
        assert_eq!(league_id, "170874");
        assert_eq!(league_key, "469.l.170874");
    }

    #[test]
    fn resolve_synthesizes_a_key_when_a_bare_number_override_differs() {
        let mut config = Config {
            current_league: "469.l.170874".into(),
            ..Config::default()
        };
        let mut output = Vec::new();
        let (league_id, league_key) = resolve_league(
            &mut config,
            Some("999999"),
            false,
            &mut Cursor::new(""),
            &mut output,
        )
        .unwrap();
        assert_eq!(league_id, "999999");
        assert_eq!(league_key, "public.999999");
        assert_eq!(config.current_league, "public.999999");
    }

    #[test]
    fn resolve_prompts_and_saves_only_when_nothing_else_resolves() {
        let mut config = Config::default();
        let mut output = Vec::new();
        let (league_id, league_key) = resolve_league(
            &mut config,
            None,
            true,
            &mut Cursor::new("170874\n"),
            &mut output,
        )
        .unwrap();
        assert_eq!(league_id, "170874");
        assert_eq!(league_key, "public.170874");
        assert_eq!(config.pull_public_league_id, "170874");
        assert_eq!(config.current_league, "public.170874");
        assert!(String::from_utf8(output).unwrap().contains("League id:"));
    }

    #[test]
    fn resolve_fails_noninteractively_instead_of_hanging() {
        let mut config = Config::default();
        let mut output = Vec::new();
        let error = resolve_league(&mut config, None, false, &mut Cursor::new(""), &mut output)
            .unwrap_err();
        assert!(error.to_string().contains("b9 pp -l"));
        assert!(output.is_empty());
    }
}
