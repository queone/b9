//! Foreground orchestration for Yahoo-network-independent MLB utility commands.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde::{Serialize, de::DeserializeOwned};

use crate::domain::{
    BattingStats, MlbRosterPlayer, MlbSlateRow, MlbStanding, MlbTeam, MlbTeamTotals, PitchingStats,
};
use crate::mlb_display::{render_rosters, render_slate, render_totals};
use crate::providers::mlb::{
    BulkHittingSplit, BulkPitchingSplit, MlbClient, PrimaryType, ScheduleGame, TeamDirectoryEntry,
};
use crate::providers::oddsshark::OddsSharkClient;
use crate::store::{RosterWrite, SeasonStatWrite, Store, SyncMode, SyncOrigin};
use crate::terminal::detected_help_color_mode;
use crate::transport::HttpClient;

const DIRECTORY_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const ROSTER_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const TOTALS_TTL: Duration = Duration::from_secs(15 * 60);
const SLATE_TTL: Duration = Duration::from_secs(60);

/// One contextual MLB command failure.
#[derive(Debug)]
pub struct MlbCommandError(String);
impl fmt::Display for MlbCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for MlbCommandError {}
fn error(command: &str, operation: &str, detail: impl fmt::Display) -> MlbCommandError {
    MlbCommandError(format!(
        "{command}: {operation}: {detail}; verify connectivity and retry"
    ))
}

/// Acquire and render MLB rosters.
pub fn show_teams(query: Option<&str>, force: bool) -> Result<String, MlbCommandError> {
    let (http, mut store, date) = production("team")?;
    let season = year(&date)?;
    let mlb = MlbClient::production(http);
    let (teams, _, directory_refreshed) = cached(
        &mut store,
        "mlb_team_directory",
        "mlb",
        &season.to_string(),
        DIRECTORY_TTL,
        force,
        || {
            mlb.fetch_team_directory(season)
                .map(|rows| rows.into_iter().map(team).collect::<Vec<_>>())
        },
    )?;
    let selected = resolve_teams_interactively(&teams, query)?;
    let games = mlb.fetch_schedule(&date).unwrap_or_default();
    let (records, records_refreshed) = match cached(
        &mut store,
        "mlb_team_records",
        "mlb",
        &season.to_string(),
        TOTALS_TTL,
        force,
        || mlb.fetch_standings(season),
    ) {
        Ok((rows, _, refreshed)) => (
            rows.into_iter()
                .map(|row| (row.team_id, (row.wins, row.losses)))
                .collect::<HashMap<_, _>>(),
            refreshed,
        ),
        Err(_) => (HashMap::new(), false),
    };
    let mut groups = Vec::new();
    let mut warnings = Vec::new();
    let ownership_synced_at = store
        .ownership_synced_at()
        .map_err(|failure| error("team", "read ownership freshness", failure))?;
    let ownership_age_days = ownership_synced_at.map(|value| {
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH + Duration::from_secs(value.max(0) as u64))
            .unwrap_or_default()
            .as_secs()
            / 86_400
    });
    if ownership_age_days.is_none_or(|days| days >= 1) {
        let age = ownership_age_days
            .map(|days| format!("{days}d ago"))
            .unwrap_or_else(|| "never".into());
        warnings.push(format!(
            "OWNER data last synced {age} — run `b9 sync` to refresh."
        ));
    }
    let mut refreshed = directory_refreshed || records_refreshed;
    for club in selected {
        let scope = format!("{}:{}", season, club.abbreviation);
        match cached(
            &mut store,
            "mlb_team_roster",
            "mlb",
            &scope,
            ROSTER_TTL,
            force,
            || {
                let rows = mlb.fetch_roster(club.id)?;
                Ok::<_, crate::providers::ProviderError>(
                    rows.into_iter()
                        .map(|row| MlbRosterPlayer {
                            team_abbreviation: club.abbreviation.clone(),
                            mlbam_id: row.person_id,
                            name: row.full_name,
                            position: row.position,
                            primary_type: match row.primary_type {
                                PrimaryType::H => "H",
                                PrimaryType::P => "P",
                            }
                            .into(),
                            status: row.status,
                            injury_status: String::new(),
                            game_status: String::new(),
                            is_closer: false,
                            jersey_number: row.jersey_number,
                            eligible_positions: String::new(),
                            bat_side: String::new(),
                            pitch_hand: String::new(),
                            yahoo_rank: None,
                            owner: None,
                            in_yahoo_pool: false,
                            plate_appearances: 0,
                            on_base_percentage: 0.0,
                            runs: 0,
                            home_runs: 0,
                            runs_batted_in: 0,
                            stolen_bases: 0,
                            batting_average: 0.0,
                            innings_pitched: 0.0,
                            quality_starts: 0,
                            wins: 0,
                            saves: 0,
                            strikeouts: 0,
                            earned_run_average: 0.0,
                            whip: 0.0,
                        })
                        .collect::<Vec<_>>(),
                )
            },
        ) {
            Ok((players, stale, roster_refreshed)) => {
                refreshed |= roster_refreshed;
                let writes = players
                    .iter()
                    .map(|row| RosterWrite {
                        mlbam_id: row.mlbam_id,
                        name: row.name.clone(),
                        position: row.position.clone(),
                        primary_type: row.primary_type.clone(),
                        status: row.status.clone(),
                        jersey_number: row.jersey_number.clone(),
                    })
                    .collect::<Vec<_>>();
                store
                    .replace_mlb_roster(&club.abbreviation, &writes)
                    .map_err(|failure| error("team", "save roster", failure))?;
                if stale {
                    warnings.push(format!("{} roster is stale", club.abbreviation));
                }
                let players = store
                    .mlb_roster(&club.abbreviation)
                    .map_err(|failure| error("team", "read enriched roster", failure))?
                    .into_iter()
                    .map(|row| {
                        let game_status = team_player_game_status(
                            row.mlbam_id,
                            &row.primary_type,
                            club.id,
                            &games,
                            &teams,
                        );
                        MlbRosterPlayer {
                            team_abbreviation: club.abbreviation.clone(),
                            mlbam_id: row.mlbam_id,
                            name: row.name,
                            position: row.position,
                            primary_type: row.primary_type,
                            status: row.status,
                            injury_status: row.injury_status,
                            game_status,
                            is_closer: row.is_closer,
                            jersey_number: row.jersey_number,
                            eligible_positions: row.eligible_positions,
                            bat_side: row.bat_side,
                            pitch_hand: row.pitch_hand,
                            yahoo_rank: row.yahoo_rank,
                            owner: row.owner,
                            in_yahoo_pool: row.in_yahoo_pool,
                            plate_appearances: row.plate_appearances,
                            on_base_percentage: row.on_base_percentage,
                            runs: row.runs,
                            home_runs: row.home_runs,
                            runs_batted_in: row.runs_batted_in,
                            stolen_bases: row.stolen_bases,
                            batting_average: row.batting_average,
                            innings_pitched: row.innings_pitched,
                            quality_starts: row.quality_starts,
                            wins: row.wins,
                            saves: row.saves,
                            strikeouts: row.strikeouts,
                            earned_run_average: row.earned_run_average,
                            whip: row.whip,
                        }
                    })
                    .collect();
                let record = records
                    .get(&club.id)
                    .map(|(wins, losses)| format!(" ({wins}-{losses})"))
                    .unwrap_or_default();
                groups.push((
                    format!("{} - {}{record}", club.abbreviation, club.name),
                    players,
                ));
            }
            Err(failure) => warnings.push(format!(
                "{} roster unavailable: {failure}",
                club.abbreviation
            )),
        }
    }
    if groups.is_empty() {
        return Err(error(
            "team",
            "load rosters",
            "no requested team has usable roster data",
        ));
    }
    record_run(&mut store, refreshed, "rosters", groups.len() as i64)?;
    Ok(render_rosters(
        &groups,
        &warnings,
        detected_help_color_mode(),
    ))
}

/// Acquire and render MLB standings and totals.
pub fn show_totals(force: bool) -> Result<String, MlbCommandError> {
    let (http, mut store, date) = production("team totals")?;
    let season = year(&date)?;
    let mlb = MlbClient::production(http);
    let (teams, _, directory_refreshed) = cached(
        &mut store,
        "mlb_team_directory",
        "mlb",
        &season.to_string(),
        DIRECTORY_TTL,
        force,
        || {
            mlb.fetch_team_directory(season)
                .map(|rows| rows.into_iter().map(team).collect::<Vec<_>>())
        },
    )?;
    let scope = season.to_string();
    let ((standings, mut totals, writes), stale, totals_refreshed) = cached(
        &mut store,
        "mlb_team_totals",
        "mlb",
        &scope,
        TOTALS_TTL,
        force,
        || {
            let standings = mlb.fetch_standings(season)?;
            let hitting = mlb.fetch_bulk_hitting_stats(season, "R")?;
            let pitching = mlb.fetch_bulk_pitching_stats(season, "R")?;
            let team_map = teams
                .iter()
                .map(|team| (team.id, team.clone()))
                .collect::<HashMap<_, _>>();
            let standings = standings
                .into_iter()
                .filter_map(|row| {
                    team_map.get(&row.team_id).cloned().map(|team| MlbStanding {
                        team,
                        wins: row.wins,
                        losses: row.losses,
                        games_back: row.games_back,
                    })
                })
                .collect::<Vec<_>>();
            let totals = aggregate(&teams, &hitting, &pitching);
            let writes = stat_writes(&teams, &hitting, &pitching);
            Ok::<_, crate::providers::ProviderError>((standings, totals, writes))
        },
    )?;
    if !writes.is_empty() {
        store
            .replace_mlb_season_stats(season, &writes)
            .map_err(|failure| error("team totals", "save season totals", failure))?;
    }
    let local_counts = store
        .mlb_local_player_counts()
        .map_err(|failure| error("team totals", "read local Yahoo context", failure))?;
    for total in &mut totals {
        if let Some((rostered, available)) = local_counts.get(&total.team.abbreviation) {
            total.yahoo_players = Some(*rostered);
            total.players_available = Some(*available);
        }
        total.pitching.quality_starts = store
            .mlb_roster(&total.team.abbreviation)
            .map_err(|failure| error("team totals", "read synchronized quality starts", failure))?
            .iter()
            .filter(|row| row.primary_type == "P")
            .map(|row| row.quality_starts)
            .sum::<i64>() as i32;
    }
    record_run(
        &mut store,
        directory_refreshed || totals_refreshed,
        "teams",
        totals.len() as i64,
    )?;
    Ok(render_totals(
        &standings,
        &totals,
        stale,
        detected_help_color_mode(),
    ))
}

/// Acquire and render the three-day probable-pitcher slate.
pub fn show_probables(force: bool) -> Result<String, MlbCommandError> {
    let (http, mut store, today) = production("probable pitchers")?;
    let season = year(&today)?;
    let mlb = MlbClient::production(http.clone());
    let (teams, _, directory_refreshed) = cached(
        &mut store,
        "mlb_team_directory",
        "mlb",
        &season.to_string(),
        DIRECTORY_TTL,
        force,
        || {
            mlb.fetch_team_directory(season)
                .map(|rows| rows.into_iter().map(team).collect::<Vec<_>>())
        },
    )?;
    let dates = [today.clone(), add_days(&today, 1)?, add_days(&today, 2)?];
    let (current, current_stale, current_refreshed) = match cached(
        &mut store,
        "mlb_current_odds",
        "espn",
        &today,
        Duration::from_secs(30 * 60),
        force,
        || {
            crate::providers::espn::EspnClient::production(http.clone())
                .fetch_game_lines(SystemTime::now())
        },
    ) {
        Ok(outcome) => outcome,
        Err(_) => (
            crate::providers::espn::SlateLines {
                games: Vec::new(),
                issues: Vec::new(),
            },
            true,
            false,
        ),
    };
    let future_client = OddsSharkClient::production(http.clone());
    let mut future_lines = BTreeMap::new();
    let mut odds_stale = current_stale;
    let mut odds_refreshed = current_refreshed;
    for date in dates.iter().skip(1) {
        match cached(
            &mut store,
            "mlb_future_odds",
            "oddsshark",
            date,
            Duration::from_secs(12 * 60 * 60),
            force,
            || future_client.fetch_game_lines(date),
        ) {
            Ok((lines, stale, refreshed)) => {
                future_lines.insert(date.clone(), lines);
                odds_stale |= stale;
                odds_refreshed |= refreshed;
            }
            Err(_) => odds_stale = true,
        }
    }
    let scope = today.clone();
    let (mut rows, stale, slate_refreshed) = cached(
        &mut store,
        "mlb_probable_pitchers",
        "mlb",
        &scope,
        SLATE_TTL,
        force,
        || {
            let mut games = Vec::new();
            let mut official_dates = HashMap::new();
            for date in &dates {
                let scheduled = mlb.fetch_schedule(date)?;
                official_dates.extend(scheduled.iter().map(|game| (game.game_id, date.clone())));
                games.extend(scheduled);
            }
            Ok::<_, crate::providers::ProviderError>(slate_rows(
                &games,
                &teams,
                &today,
                &official_dates,
                Some(&current),
                &future_lines,
            ))
        },
    )?;
    let current_team_key = crate::config::read()
        .map(|config| config.current_team_key)
        .unwrap_or_default();
    let ownership = store
        .mlb_local_pitcher_ownership(&current_team_key)
        .map_err(|failure| error("probable pitchers", "read local ownership", failure))?;
    for row in &mut rows {
        if let Some((in_pool, rostered, mine)) = ownership.get(&row.away_pitcher.to_lowercase()) {
            row.away_free_agent = *in_pool && !*rostered;
            row.away_mine = *mine;
        }
        if let Some((in_pool, rostered, mine)) = ownership.get(&row.home_pitcher.to_lowercase()) {
            row.home_free_agent = *in_pool && !*rostered;
            row.home_mine = *mine;
        }
    }
    let warnings = slate_warnings(stale, odds_stale);
    record_run(
        &mut store,
        directory_refreshed || slate_refreshed || odds_refreshed,
        "slate_rows",
        rows.len() as i64,
    )?;
    Ok(render_slate(&rows, &warnings, detected_help_color_mode()))
}

fn production(command: &str) -> Result<(Arc<HttpClient>, Store, String), MlbCommandError> {
    let http = Arc::new(
        HttpClient::production()
            .map_err(|failure| error(command, "initialize HTTP transport", failure))?,
    );
    let store = Store::open().map_err(|failure| error(command, "open database", failure))?;
    let output = Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .map_err(|failure| error(command, "read host-local date", failure))?;
    let date = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || date.len() != 10 {
        return Err(error(
            command,
            "read host-local date",
            "date utility returned an invalid value",
        ));
    }
    Ok((http, store, date))
}

fn team_player_game_status(
    mlbam_id: i64,
    primary_type: &str,
    team_id: i64,
    games: &[ScheduleGame],
    teams: &[MlbTeam],
) -> String {
    let abbreviations = teams
        .iter()
        .map(|team| (team.id, team.abbreviation.as_str()))
        .collect::<HashMap<_, _>>();
    let Some(game) = games
        .iter()
        .find(|game| game.away_team_id == team_id || game.home_team_id == team_id)
    else {
        return String::new();
    };
    let away = game.away_team_id == team_id;
    let opponent_id = if away {
        game.home_team_id
    } else {
        game.away_team_id
    };
    let opponent = abbreviations.get(&opponent_id).copied().unwrap_or("—");
    let marker = if away { "@" } else { "v" };
    let state = game.detailed_state.to_ascii_lowercase();
    if state == "final" {
        return format!("Final {marker} {opponent}");
    }
    if !matches!(
        state.as_str(),
        "scheduled" | "pre-game" | "pregame" | "warmup"
    ) {
        return format!("Live {marker} {opponent}");
    }
    let indicator = if primary_type == "P" {
        let probable = if away {
            game.away_probable_pitcher_id
        } else {
            game.home_probable_pitcher_id
        };
        if probable == Some(mlbam_id) {
            "●".to_owned()
        } else {
            String::new()
        }
    } else {
        let lineup = if away {
            game.away_lineup.as_deref()
        } else {
            game.home_lineup.as_deref()
        };
        lineup.map_or_else(String::new, |lineup| {
            lineup
                .iter()
                .position(|entry| entry.person_id == mlbam_id)
                .map_or_else(
                    || {
                        if lineup.is_empty() {
                            String::new()
                        } else {
                            "●".to_owned()
                        }
                    },
                    |index| (index + 1).to_string(),
                )
        })
    };
    format!(
        "{} {indicator:1} {marker} {opponent}",
        host_local_game_time(&game.game_date)
    )
}

fn cached<T, E, F>(
    store: &mut Store,
    dataset: &str,
    source: &str,
    scope: &str,
    ttl: Duration,
    force: bool,
    fetch: F,
) -> Result<(T, bool, bool), MlbCommandError>
where
    T: Serialize + DeserializeOwned,
    E: fmt::Display,
    F: FnOnce() -> Result<T, E>,
{
    let previous = store
        .command_snapshot(dataset, source, scope)
        .map_err(|failure| error(dataset, "read snapshot", failure))?;
    if !force
        && let Some(snapshot) = &previous
        && !snapshot.stale
        && SystemTime::now()
            .duration_since(snapshot.last_successful_at)
            .unwrap_or_default()
            < ttl
        && let Ok(value) = serde_json::from_str(&snapshot.payload)
    {
        return Ok((value, false, false));
    }
    match fetch() {
        Ok(value) => {
            let payload = serde_json::to_string(&value)
                .map_err(|failure| error(dataset, "encode snapshot", failure))?;
            store
                .save_command_snapshot(dataset, source, scope, "v1", &payload)
                .map_err(|failure| error(dataset, "save snapshot", failure))?;
            Ok((value, false, true))
        }
        Err(failure) => {
            if let Some(snapshot) = previous {
                let _ =
                    store.mark_command_snapshot_stale(dataset, source, scope, &failure.to_string());
                let value = serde_json::from_str(&snapshot.payload)
                    .map_err(|decode| error(dataset, "decode stale snapshot", decode))?;
                Ok((value, true, false))
            } else {
                Err(error(dataset, "refresh data", failure))
            }
        }
    }
}

fn record_run(
    store: &mut Store,
    refreshed: bool,
    key: &str,
    count: i64,
) -> Result<(), MlbCommandError> {
    if !refreshed {
        return Ok(());
    }
    let id = store
        .start_sync_run(SyncMode::Live, SyncOrigin::Manual)
        .map_err(|failure| error("mlb", "start foreground refresh record", failure))?;
    let counts = [(key.to_owned(), count)].into_iter().collect();
    store
        .complete_sync_run(id, &counts)
        .map_err(|failure| error("mlb", "complete foreground refresh record", failure))?;
    Ok(())
}

fn team(row: TeamDirectoryEntry) -> MlbTeam {
    MlbTeam {
        id: row.team_id,
        name: row.name,
        location: row.location_name,
        club_name: row.club_name,
        abbreviation: row.abbreviation,
        league_id: row.league_id,
    }
}
fn resolve_teams(teams: &[MlbTeam], query: Option<&str>) -> Result<Vec<MlbTeam>, MlbCommandError> {
    let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(teams.to_vec());
    };
    let folded = query.to_lowercase();
    if let Some(team) = teams
        .iter()
        .find(|team| team.abbreviation.eq_ignore_ascii_case(query))
    {
        return Ok(vec![team.clone()]);
    }
    let matches = teams
        .iter()
        .filter(|team| {
            team.location.to_lowercase().contains(&folded)
                || team.club_name.to_lowercase().contains(&folded)
        })
        .cloned()
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [team] => Ok(vec![team.clone()]),
        [] => Err(error(
            "team",
            "resolve team",
            format!("no MLB club matches {query:?}"),
        )),
        _ => Err(error(
            "team",
            "resolve team",
            format!(
                "{query:?} is ambiguous; matches: {}",
                matches
                    .iter()
                    .map(|team| team.abbreviation.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

fn resolve_teams_interactively(
    teams: &[MlbTeam],
    query: Option<&str>,
) -> Result<Vec<MlbTeam>, MlbCommandError> {
    let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) else {
        return resolve_teams(teams, query);
    };
    let folded = query.to_lowercase();
    let matches = teams
        .iter()
        .filter(|team| {
            team.abbreviation.eq_ignore_ascii_case(query)
                || team.location.to_lowercase().contains(&folded)
                || team.club_name.to_lowercase().contains(&folded)
        })
        .cloned()
        .collect::<Vec<_>>();
    if matches.len() < 2 || !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return resolve_teams(teams, Some(query));
    }
    let mut stderr = io::stderr().lock();
    writeln!(stderr, "team: {query:?} matches multiple MLB clubs:")
        .map_err(|failure| error("team", "show team choices", failure))?;
    for (index, team) in matches.iter().enumerate() {
        writeln!(
            stderr,
            "  {}) {} — {}",
            index + 1,
            team.abbreviation,
            team.name
        )
        .map_err(|failure| error("team", "show team choices", failure))?;
    }
    write!(stderr, "Select a team [1-{}]: ", matches.len())
        .and_then(|()| stderr.flush())
        .map_err(|failure| error("team", "prompt for team", failure))?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|failure| error("team", "read team selection", failure))?;
    let selection = answer
        .trim()
        .parse::<usize>()
        .ok()
        .and_then(|number| number.checked_sub(1).and_then(|index| matches.get(index)));
    selection.cloned().map(|team| vec![team]).ok_or_else(|| {
        error(
            "team",
            "resolve team",
            format!(
                "invalid selection; enter a number from 1 through {}",
                matches.len()
            ),
        )
    })
}

fn aggregate(
    teams: &[MlbTeam],
    hitting: &[BulkHittingSplit],
    pitching: &[BulkPitchingSplit],
) -> Vec<MlbTeamTotals> {
    teams
        .iter()
        .map(|team| {
            let hs = hitting
                .iter()
                .filter(|row| row.team.team_id == team.id)
                .map(|row| &row.stat)
                .collect::<Vec<_>>();
            let ps = pitching
                .iter()
                .filter(|row| row.team.team_id == team.id)
                .map(|row| &row.stat)
                .collect::<Vec<_>>();
            let ab: i64 = hs.iter().map(|s| s.at_bats).sum();
            let hits: i64 = hs.iter().map(|s| s.hits).sum();
            let walks: i64 = hs.iter().map(|s| s.walks).sum();
            let hbp: i64 = hs.iter().map(|s| s.hit_by_pitch).sum();
            let tb: i64 = hs.iter().map(|s| s.total_bases).sum();
            let avg = ratio(hits, ab);
            let obp = ratio(hits + walks + hbp, ab + walks + hbp);
            let slg = ratio(tb, ab);
            let outs: i64 = ps.iter().map(|s| innings_outs(&s.innings_pitched)).sum();
            let innings = outs as f64 / 3.0;
            let er: i64 = ps.iter().map(|s| s.earned_runs).sum();
            let ph: i64 = ps.iter().map(|s| s.hits_allowed).sum();
            let pbb: i64 = ps.iter().map(|s| s.walks).sum();
            MlbTeamTotals {
                team: team.clone(),
                batting: BattingStats {
                    plate_appearances: hs.iter().map(|s| s.plate_appearances).sum::<i64>() as i32,
                    batting_average: avg,
                    on_base_percentage: obp,
                    slugging_percentage: slg,
                    on_base_plus_slugging: obp + slg,
                    home_runs: hs.iter().map(|s| s.home_runs).sum::<i64>() as i32,
                    runs_batted_in: hs.iter().map(|s| s.rbi).sum::<i64>() as i32,
                    runs: hs.iter().map(|s| s.runs).sum::<i64>() as i32,
                    stolen_bases: hs.iter().map(|s| s.stolen_bases).sum::<i64>() as i32,
                    strikeouts: hs.iter().map(|s| s.strikeouts).sum::<i64>() as i32,
                    walks: walks as i32,
                },
                pitching: PitchingStats {
                    games: ps.iter().map(|s| s.games_pitched).sum::<i64>() as i32,
                    games_started: ps.iter().map(|s| s.games_started).sum::<i64>() as i32,
                    innings_pitched: innings,
                    earned_run_average: if innings == 0.0 {
                        0.0
                    } else {
                        9.0 * er as f64 / innings
                    },
                    whip: if innings == 0.0 {
                        0.0
                    } else {
                        (ph + pbb) as f64 / innings
                    },
                    strikeouts: ps.iter().map(|s| s.strikeouts).sum::<i64>() as i32,
                    wins: ps.iter().map(|s| s.wins).sum::<i64>() as i32,
                    saves: ps.iter().map(|s| s.saves).sum::<i64>() as i32,
                    holds: ps.iter().map(|s| s.holds).sum::<i64>() as i32,
                    quality_starts: ps.iter().map(|s| s.quality_starts).sum::<i64>() as i32,
                    ..PitchingStats::default()
                },
                yahoo_players: None,
                players_available: None,
            }
        })
        .collect()
}

fn stat_writes(
    teams: &[MlbTeam],
    hitting: &[BulkHittingSplit],
    pitching: &[BulkPitchingSplit],
) -> Vec<SeasonStatWrite> {
    let abbreviations = teams
        .iter()
        .map(|team| (team.id, team.abbreviation.clone()))
        .collect::<HashMap<_, _>>();
    let mut rows = hitting
        .iter()
        .filter_map(|row| {
            abbreviations.get(&row.team.team_id).map(|team| {
                let stat = &row.stat;
                SeasonStatWrite {
                    mlbam_id: row.player.person_id,
                    name: row.player.full_name.clone(),
                    team_abbreviation: team.clone(),
                    stat_group: "hitting".into(),
                    games: stat.games_played,
                    plate_appearances: stat.plate_appearances,
                    at_bats: stat.at_bats,
                    hits: stat.hits,
                    home_runs: stat.home_runs,
                    runs_batted_in: stat.rbi,
                    runs: stat.runs,
                    stolen_bases: stat.stolen_bases,
                    walks: stat.walks,
                    hit_by_pitch: stat.hit_by_pitch,
                    total_bases: stat.total_bases,
                    ..SeasonStatWrite::default()
                }
            })
        })
        .collect::<Vec<_>>();
    rows.extend(pitching.iter().filter_map(|row| {
        abbreviations.get(&row.team.team_id).map(|team| {
            let stat = &row.stat;
            SeasonStatWrite {
                mlbam_id: row.player.person_id,
                name: row.player.full_name.clone(),
                team_abbreviation: team.clone(),
                stat_group: "pitching".into(),
                games: stat.games_pitched,
                wins: stat.wins,
                saves: stat.saves,
                holds: stat.holds,
                strikeouts: stat.strikeouts,
                innings_outs: innings_outs(&stat.innings_pitched),
                games_started: stat.games_started,
                quality_starts: stat.quality_starts,
                hits_allowed: stat.hits_allowed,
                earned_runs: stat.earned_runs,
                pitcher_walks: stat.walks,
                ..SeasonStatWrite::default()
            }
        })
    }));
    rows.sort_by(|left, right| {
        (left.mlbam_id, left.stat_group.as_str()).cmp(&(right.mlbam_id, right.stat_group.as_str()))
    });
    rows
}

fn ratio(n: i64, d: i64) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 }
}
fn innings_outs(value: &str) -> i64 {
    let mut parts = value.split('.');
    let whole = parts
        .next()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    let rem = parts
        .next()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
        .min(2);
    whole * 3 + rem
}

fn slate_rows(
    games: &[ScheduleGame],
    teams: &[MlbTeam],
    today: &str,
    official_dates: &HashMap<i64, String>,
    espn: Option<&crate::providers::espn::SlateLines>,
    future: &BTreeMap<String, Vec<crate::providers::oddsshark::GameLine>>,
) -> Vec<MlbSlateRow> {
    let names = teams
        .iter()
        .map(|t| (t.id, t.abbreviation.clone()))
        .collect::<HashMap<_, _>>();
    let mut rows = Vec::new();
    let mut ordered_games = games.iter().collect::<Vec<_>>();
    ordered_games.sort_by(|left, right| {
        (&left.game_date, left.game_id).cmp(&(&right.game_date, right.game_id))
    });
    for game in ordered_games {
        let date = game_official_date(game, official_dates);
        let away = names
            .get(&game.away_team_id)
            .cloned()
            .unwrap_or_else(|| game.away_team_name.clone());
        let home = names
            .get(&game.home_team_id)
            .cloned()
            .unwrap_or_else(|| game.home_team_name.clone());
        let occurrence = games
            .iter()
            .take_while(|candidate| candidate.game_id != game.game_id)
            .filter(|candidate| {
                game_official_date(candidate, official_dates) == date
                    && same_team(&candidate.away_team_name, &game.away_team_name)
                    && same_team(&candidate.home_team_name, &game.home_team_name)
            })
            .count();
        let probs = if date == today {
            espn.and_then(|lines| {
                lines
                    .games
                    .iter()
                    .filter(|line| {
                        same_team(&line.away_team, &game.away_team_name)
                            && same_team(&line.home_team, &game.home_team_name)
                            && line.quoted
                    })
                    .nth(occurrence)
                    .map(|line| normalized(line.away_moneyline, line.home_moneyline))
            })
        } else {
            future.get(&date).and_then(|lines| {
                lines
                    .iter()
                    .find(|line| {
                        line.event_id == game.game_id.to_string()
                            || (!line.start_time.is_empty() && line.start_time == game.game_date)
                    })
                    .or_else(|| {
                        lines
                            .iter()
                            .filter(|line| {
                                same_team(&line.away_team, &game.away_team_name)
                                    && same_team(&line.home_team, &game.home_team_name)
                            })
                            .nth(occurrence)
                    })
                    .map(|line| normalized(line.away_moneyline, line.home_moneyline))
            })
        };
        rows.push(MlbSlateRow {
            date: compact_date(&date),
            game_id: game.game_id,
            game_time: host_local_game_time(&game.game_date),
            away_team: away,
            home_team: home,
            away_pitcher: blank(&game.away_probable_pitcher_name),
            home_pitcher: blank(&game.home_probable_pitcher_name),
            win_probability: probs.map(|p| p.0),
            away_free_agent: false,
            home_free_agent: false,
            away_mine: false,
            home_mine: false,
        });
    }
    rows
}

fn game_official_date(game: &ScheduleGame, dates: &HashMap<i64, String>) -> String {
    dates
        .get(&game.game_id)
        .cloned()
        .unwrap_or_else(|| game.game_date.chars().take(10).collect())
}

fn slate_warnings(slate_stale: bool, odds_stale: bool) -> Vec<String> {
    let mut warnings = Vec::new();
    if slate_stale {
        warnings.push("probable-pitcher slate is stale after MLB provider degradation".into());
    }
    if odds_stale {
        warnings.push("odds are stale or unavailable after provider degradation".into());
    }
    warnings
}
fn blank(value: &str) -> String {
    if value.trim().is_empty() {
        "TBD".into()
    } else {
        value.into()
    }
}
pub(crate) fn host_local_game_time(value: &str) -> String {
    let utc_value = value
        .strip_suffix('Z')
        .map_or_else(|| value.to_owned(), |value| format!("{value}+0000"));
    let mac = Command::new("date")
        .args(["-j", "-f", "%Y-%m-%dT%H:%M:%S%z", &utc_value, "+%I:%M%p"])
        .output();
    if let Ok(output) = mac
        && output.status.success()
    {
        let rendered = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !rendered.is_empty() {
            return rendered
                .trim_start_matches('0')
                .to_lowercase()
                .replace("am", "a")
                .replace("pm", "p");
        }
    }
    let linux = Command::new("date")
        .args(["-d", value, "+%I:%M%P"])
        .output();
    if let Ok(output) = linux
        && output.status.success()
    {
        let rendered = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !rendered.is_empty() {
            return rendered
                .trim_start_matches('0')
                .replace("am", "a")
                .replace("pm", "p");
        }
    }
    value.to_owned()
}
fn compact_date(value: &str) -> String {
    for arguments in [
        vec!["-j", "-f", "%Y-%m-%d", value, "+%b %d %a"],
        vec!["-d", value, "+%b %d %a"],
    ] {
        if let Ok(output) = Command::new("date").args(arguments).output()
            && output.status.success()
        {
            let rendered = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !rendered.is_empty() {
                return rendered;
            }
        }
    }
    value.to_owned()
}
fn same_team(a: &str, b: &str) -> bool {
    let fold = |v: &str| {
        v.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    fold(a) == fold(b)
}
fn normalized(a: i64, h: i64) -> (f64, f64) {
    let p = |v: i64| {
        if v > 0 {
            100.0 / (v as f64 + 100.0)
        } else if v < 0 {
            (-v) as f64 / ((-v) as f64 + 100.0)
        } else {
            0.0
        }
    };
    let a = p(a);
    let h = p(h);
    let t = a + h;
    if t == 0.0 { (0.0, 0.0) } else { (a / t, h / t) }
}
fn year(date: &str) -> Result<i64, MlbCommandError> {
    date.get(..4)
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| error("mlb", "parse season", date))
}
fn add_days(date: &str, days: i64) -> Result<String, MlbCommandError> {
    let y = year(date)? as i32;
    let m = date
        .get(5..7)
        .and_then(|v| v.parse::<u32>().ok())
        .ok_or_else(|| error("mlb", "parse date", date))?;
    let d = date
        .get(8..10)
        .and_then(|v| v.parse::<u32>().ok())
        .ok_or_else(|| error("mlb", "parse date", date))?;
    let z = days_from_civil(y, m, d) + days;
    let (y, m, d) = civil_from_days(z);
    Ok(format!("{y:04}-{m:02}-{d:02}"))
}
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::mlb::{
        BulkPlayer, BulkPosition, BulkTeam, HittingStats, PitchingStats as ProviderPitching,
    };
    use tempfile::tempdir;

    fn club(id: i64, abbreviation: &str, location: &str, name: &str) -> MlbTeam {
        MlbTeam {
            id,
            name: format!("{location} {name}"),
            location: location.into(),
            club_name: name.into(),
            abbreviation: abbreviation.into(),
            league_id: 103,
        }
    }

    #[test]
    fn team_resolution_dates_and_aggregate_rates_are_deterministic() {
        let teams = vec![
            club(1, "NYY", "New York", "Yankees"),
            club(2, "NYM", "New York", "Mets"),
        ];
        assert_eq!(resolve_teams(&teams, Some("nyy")).unwrap()[0].id, 1);
        assert_eq!(resolve_teams(&teams, Some("mets")).unwrap()[0].id, 2);
        assert!(resolve_teams(&teams, Some("new york")).is_err());
        assert_eq!(add_days("2024-02-28", 1).unwrap(), "2024-02-29");
        assert_eq!(add_days("2026-12-31", 1).unwrap(), "2027-01-01");
        let hitting = vec![BulkHittingSplit {
            player: BulkPlayer {
                person_id: 1,
                full_name: "Hitter".into(),
            },
            team: BulkTeam { team_id: 1 },
            position: BulkPosition::default(),
            stat: HittingStats {
                plate_appearances: 13,
                at_bats: 10,
                hits: 3,
                walks: 2,
                hit_by_pitch: 1,
                total_bases: 5,
                ..HittingStats::default()
            },
        }];
        let pitching = vec![BulkPitchingSplit {
            player: BulkPlayer {
                person_id: 2,
                full_name: "Pitcher".into(),
            },
            team: BulkTeam { team_id: 1 },
            position: BulkPosition::default(),
            stat: ProviderPitching {
                innings_pitched: "6.2".into(),
                earned_runs: 2,
                hits_allowed: 5,
                walks: 2,
                ..ProviderPitching::default()
            },
        }];
        let totals = aggregate(&teams[..1], &hitting, &pitching);
        assert!((totals[0].batting.batting_average - 0.3).abs() < 1e-9);
        assert!((totals[0].batting.on_base_percentage - 6.0 / 13.0).abs() < 1e-9);
        assert!((totals[0].pitching.innings_pitched - 20.0 / 3.0).abs() < 1e-9);
        assert!((totals[0].pitching.earned_run_average - 2.7).abs() < 1e-9);
    }

    #[test]
    fn snapshots_honor_fresh_force_and_stale_fallback_states() {
        let directory = tempdir().unwrap();
        let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
        let mut calls = 0;
        let first: (Vec<i32>, _, _) = cached(
            &mut store,
            "test_mlb",
            "mlb",
            "scope",
            Duration::from_secs(60),
            false,
            || {
                calls += 1;
                Ok::<_, &str>(vec![1])
            },
        )
        .unwrap();
        assert_eq!(first, (vec![1], false, true));
        let fresh = cached(
            &mut store,
            "test_mlb",
            "mlb",
            "scope",
            Duration::from_secs(60),
            false,
            || {
                calls += 1;
                Ok::<_, &str>(vec![2])
            },
        )
        .unwrap();
        assert_eq!(fresh, (vec![1], false, false));
        let stale = cached(
            &mut store,
            "test_mlb",
            "mlb",
            "scope",
            Duration::from_secs(60),
            true,
            || Err::<Vec<i32>, _>("offline"),
        )
        .unwrap();
        assert_eq!(stale, (vec![1], true, false));
        assert_eq!(calls, 1);
    }

    #[test]
    fn probable_slate_uses_mlb_official_date_across_utc_midnight() {
        let game = ScheduleGame {
            game_id: 1,
            game_date: "2026-08-19T00:40:00Z".into(),
            detailed_state: "Scheduled".into(),
            away_team_id: 1,
            away_team_name: "Los Angeles Dodgers".into(),
            home_team_id: 2,
            home_team_name: "Colorado Rockies".into(),
            away_probable_pitcher_id: Some(10),
            away_probable_pitcher_name: "Eric Lauer".into(),
            home_probable_pitcher_id: Some(20),
            home_probable_pitcher_name: "Ryan Feltner".into(),
            linescore: None,
            away_lineup: None,
            home_lineup: None,
        };
        let teams = vec![
            club(1, "LAD", "Los Angeles", "Dodgers"),
            club(2, "COL", "Colorado", "Rockies"),
        ];
        let pitcher: MlbRosterPlayer = serde_json::from_str(
            r#"{"team_abbreviation":"LAD","mlbam_id":10,"name":"Eric Lauer","position":"P","primary_type":"P","status":"A","jersey_number":""}"#,
        )
        .unwrap();
        let status = team_player_game_status(
            pitcher.mlbam_id,
            &pitcher.primary_type,
            1,
            std::slice::from_ref(&game),
            &teams,
        );
        assert!(status.contains("● @ COL"));
        assert!(!status.contains("Scheduled"));
        let official_dates = HashMap::from([(1, "2026-08-18".to_owned())]);
        let rows = slate_rows(
            &[game],
            &teams,
            "2026-08-18",
            &official_dates,
            None,
            &BTreeMap::new(),
        );

        assert_eq!(rows[0].date, compact_date("2026-08-18"));
        assert_eq!(rows[0].away_pitcher, "Eric Lauer");
    }

    #[test]
    fn probable_slate_warnings_identify_the_degraded_source() {
        assert_eq!(
            slate_warnings(true, false),
            ["probable-pitcher slate is stale after MLB provider degradation"]
        );
        assert_eq!(
            slate_warnings(false, true),
            ["odds are stale or unavailable after provider degradation"]
        );
        assert_eq!(slate_warnings(false, false), Vec::<String>::new());
    }
}
