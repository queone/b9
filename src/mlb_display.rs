//! Provider-neutral deterministic MLB command rendering.

use std::collections::{BTreeMap, BTreeSet};

use crate::domain::{MlbRosterPlayer, MlbSlateRow, MlbStanding, MlbTeamTotals};
use crate::terminal::{HelpColorMode, dim, good, roster_row, table_heading, warning};

/// Render grouped 40-man rosters in the established skout information shape.
pub fn render_rosters(
    groups: &[(String, Vec<MlbRosterPlayer>)],
    warnings: &[String],
    mode: HelpColorMode,
) -> String {
    let mut output = String::new();
    for note in warnings {
        let message = if note.starts_with("OWNER data") {
            note.clone()
        } else {
            format!("WARNING — {note}")
        };
        output.push_str(&warning(&message, mode));
        output.push('\n');
    }
    for (group_index, (team, players)) in groups.iter().enumerate() {
        if group_index > 0 {
            output.push('\n');
        }
        output.push_str(&table_heading(team, mode));
        output.push('\n');
        let two_way = two_way_ids(players);
        for (role, heading, headers) in [
            (
                "H",
                "HITTER",
                "POS    STATUS             B    YR     PA    OBP    R   HR  RBI   SB    AVG  OWNER",
            ),
            (
                "P",
                "PITCHER",
                "POS    STATUS             T    YR     IP     QS    W   SV    K    ERA   WHIP  OWNER",
            ),
        ] {
            let rows = players
                .iter()
                .filter(|player| player.primary_type == role)
                .collect::<Vec<_>>();
            if rows.is_empty() && role == "P" {
                continue;
            }
            output.push_str(&table_heading(&format!("{heading:<26}  {headers}"), mode));
            output.push('\n');
            for player in rows {
                let qualifier = if two_way.contains(&player.mlbam_id) {
                    if role == "H" {
                        " (Hitter)"
                    } else {
                        " (Pitcher)"
                    }
                } else {
                    ""
                };
                let pool = if player.in_yahoo_pool { "" } else { " †" };
                let identity = fit(
                    &format!(
                        "{} {}{qualifier}{pool}",
                        player.name, player.team_abbreviation
                    ),
                    26,
                );
                let position_value = if player.eligible_positions.is_empty() {
                    player.position.clone()
                } else if player.is_closer && role == "P" {
                    format!("{}*", player.eligible_positions)
                } else {
                    player.eligible_positions.clone()
                };
                let position = fit(&position_value, 5);
                let status_value = if is_unavailable(&player.injury_status) {
                    player.injury_status.clone()
                } else if !player.game_status.is_empty() {
                    player.game_status.clone()
                } else {
                    status_label(&player.status)
                };
                let status = fit(&status_value, 17);
                let hand = if role == "H" {
                    &player.bat_side
                } else {
                    &player.pitch_hand
                };
                let yr = player
                    .yahoo_rank
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "—".into());
                let owner = match (&player.owner, player.in_yahoo_pool) {
                    (Some(owner), _) => dim(&clip(owner, 20), mode),
                    (None, true) => good("<available>", mode),
                    (None, false) => dim("<not yet in Yahoo>", mode),
                };
                let stats = if role == "H" {
                    format!(
                        "{:>5}  {:>5}  {:>3}  {:>3}  {:>3}  {:>3}  {:>5}",
                        player.plate_appearances,
                        rate(player.on_base_percentage, 3),
                        player.runs,
                        player.home_runs,
                        player.runs_batted_in,
                        player.stolen_bases,
                        rate(player.batting_average, 3)
                    )
                } else {
                    format!(
                        "{:>5}  {:>5}  {:>3}  {:>3}  {:>4}  {:>5.2}  {:>5.2}",
                        baseball_innings(player.innings_pitched),
                        player.quality_starts,
                        player.wins,
                        player.saves,
                        player.strikeouts,
                        player.earned_run_average,
                        player.whip
                    )
                };
                let row = format!(
                    "{identity}  {position}  {status}  {:<1}  {:>4}  {stats}  {owner}\n",
                    blank(hand),
                    yr
                );
                output.push_str(&roster_row(&row, &player.status, mode));
            }
        }
    }
    output
}

/// Render league and division grouped standings with inline season totals.
pub fn render_totals(
    standings: &[MlbStanding],
    totals: &[MlbTeamTotals],
    stale: bool,
    mode: HelpColorMode,
) -> String {
    let mut output = String::new();
    if stale {
        output.push_str(&warning(
            "STALE — showing the last complete MLB snapshot.",
            mode,
        ));
        output.push('\n');
    }
    for (league, label) in [(103, "American League (AL)"), (104, "National League (NL)")] {
        let league_rows = standings
            .iter()
            .filter(|row| row.team.league_id == league)
            .collect::<Vec<_>>();
        if league_rows.is_empty() {
            continue;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&table_heading(label, mode));
        output.push('\n');
        output.push_str(&table_heading("TEAM    W    L    PCT     GB   YP     PA    OBP    R   HR  RBI   SB    AVG      IP   QS    W   SV     K    ERA   WHIP", mode));
        output.push('\n');
        for division in ["East", "Central", "West"] {
            let mut rows = league_rows
                .iter()
                .filter(|row| division_for(row.team.id) == division)
                .copied()
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| {
                (
                    std::cmp::Reverse(row.wins),
                    row.losses,
                    row.team.abbreviation.clone(),
                )
            });
            if rows.is_empty() {
                continue;
            }
            output.push_str(&table_heading(division, mode));
            output.push('\n');
            for row in rows {
                let total = totals.iter().find(|total| total.team.id == row.team.id);
                let games = row.wins + row.losses;
                let pct = if games > 0 {
                    rate(row.wins as f64 / games as f64, 3)
                } else {
                    "—".into()
                };
                let yp = total
                    .and_then(|value| value.yahoo_players)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "—".into());
                let (pa, obp, r, hr, rbi, sb, avg, ip, qs, w, sv, k, era, whip) = total
                    .map(|value| {
                        let b = &value.batting;
                        let p = &value.pitching;
                        (
                            b.plate_appearances.to_string(),
                            rate_or_dash(b.on_base_percentage, 3),
                            b.runs.to_string(),
                            b.home_runs.to_string(),
                            b.runs_batted_in.to_string(),
                            b.stolen_bases.to_string(),
                            rate_or_dash(b.batting_average, 3),
                            format!("{:.1}", p.innings_pitched),
                            p.quality_starts.to_string(),
                            p.wins.to_string(),
                            p.saves.to_string(),
                            p.strikeouts.to_string(),
                            format!("{:.2}", p.earned_run_average),
                            format!("{:.2}", p.whip),
                        )
                    })
                    .unwrap_or_else(|| {
                        (
                            "—".into(),
                            "—".into(),
                            "—".into(),
                            "—".into(),
                            "—".into(),
                            "—".into(),
                            "—".into(),
                            "—".into(),
                            "—".into(),
                            "—".into(),
                            "—".into(),
                            "—".into(),
                            "—".into(),
                            "—".into(),
                        )
                    });
                output.push_str(&format!("{:<4}  {:>3}  {:>3}  {:>5}  {:>5}  {:>3}  {:>5}  {:>5}  {:>3}  {:>3}  {:>3}  {:>3}  {:>5}  {:>6}  {:>3}  {:>3}  {:>3}  {:>4}  {:>5}  {:>5}\n", row.team.abbreviation, row.wins, row.losses, pct, games_back(&row.games_back), yp, pa, obp, r, hr, rbi, sb, avg, ip, qs, w, sv, k, era, whip));
            }
        }
    }
    output
}

/// Render a three-day probable-pitcher slate with one row per game.
pub fn render_slate(rows: &[MlbSlateRow], warnings: &[String], mode: HelpColorMode) -> String {
    let mut output = String::new();
    for note in warnings {
        output.push_str(&dim(&format!("WARNING — {note}"), mode));
        output.push('\n');
    }
    let mut date = "";
    for row in rows {
        if row.date != date {
            if !date.is_empty() {
                output.push('\n');
            }
            date = &row.date;
            output.push_str(&table_heading(date, mode));
            output.push('\n');
        }
        let pct = row
            .win_probability
            .map(|value| (value * 100.0).round() as i32);
        let away_favored = pct.is_some_and(|value| value > 50);
        let home_favored = pct.is_some_and(|value| value < 50);
        let away = pitcher_cell(
            &row.away_pitcher,
            row.away_free_agent && away_favored,
            row.away_mine,
            mode,
        );
        let home = pitcher_cell(
            &row.home_pitcher,
            row.home_free_agent && home_favored,
            row.home_mine,
            mode,
        );
        let filled = pct
            .map(|value| ((value.clamp(0, 100) + 5) / 10) as usize)
            .unwrap_or(0);
        let bar = if pct.is_some() {
            format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled))
        } else {
            "░".repeat(10)
        };
        let probability = pct
            .map(|value| format!("{value}%"))
            .unwrap_or_else(|| "—%".into());
        output.push_str(&format!(
            "{away} v {home}  {:<6} {:<7}  {bar} {probability}\n",
            row.game_time,
            format!("{}@{}", row.away_team, row.home_team)
        ));
    }
    if rows.is_empty() {
        output.push_str("No MLB games are scheduled.\n");
    }
    output
}

fn two_way_ids(players: &[MlbRosterPlayer]) -> BTreeSet<i64> {
    let mut roles = BTreeMap::<i64, BTreeSet<&str>>::new();
    for player in players {
        roles
            .entry(player.mlbam_id)
            .or_default()
            .insert(&player.primary_type);
    }
    roles
        .into_iter()
        .filter_map(|(id, roles)| (roles.len() > 1).then_some(id))
        .collect()
}
fn pitcher_cell(name: &str, free_agent: bool, mine: bool, mode: HelpColorMode) -> String {
    let last = name
        .split_whitespace()
        .last()
        .filter(|value| !value.is_empty())
        .unwrap_or("TBD");
    let suffix = if free_agent { " (FA)" } else { "" };
    let value = fit(&format!("{last}{suffix}"), 16);
    if mine {
        warning(&value, mode)
    } else if free_agent {
        good(&value, mode)
    } else if last == "TBD" {
        dim(&value, mode)
    } else {
        value
    }
}
fn division_for(team_id: i64) -> &'static str {
    match team_id {
        110 | 111 | 139 | 141 | 147 | 120 | 121 | 143 | 144 | 146 => "East",
        114 | 116 | 118 | 142 | 145 | 112 | 113 | 134 | 138 | 158 => "Central",
        _ => "West",
    }
}
fn status_label(status: &str) -> String {
    match status {
        "A" => "Active".into(),
        value if value.starts_with('D') => format!("IL {value}"),
        "MIN" | "RM" => "Minors".into(),
        "" => "—".into(),
        value => value.into(),
    }
}
fn is_unavailable(status: &str) -> bool {
    let status = status.to_ascii_uppercase();
    status == "NA" || status.starts_with("IL") || status.starts_with("DL")
}
fn baseball_innings(value: f64) -> String {
    let whole = value.floor() as i64;
    let fraction = value - whole as f64;
    let outs = if (fraction - 0.1).abs() < 0.01 {
        1
    } else if (fraction - 0.2).abs() < 0.01 {
        2
    } else if fraction < 0.17 {
        0
    } else if fraction < 0.5 {
        1
    } else {
        2
    };
    format!("{whole}.{outs}")
}
fn clip(value: &str, width: usize) -> String {
    value.chars().take(width).collect()
}
fn fit(value: &str, width: usize) -> String {
    let mut value = value.chars().take(width).collect::<String>();
    value.extend(std::iter::repeat_n(
        ' ',
        width.saturating_sub(value.chars().count()),
    ));
    value
}
fn blank(value: &str) -> &str {
    if value.is_empty() { "—" } else { value }
}
fn rate(value: f64, precision: usize) -> String {
    format!("{value:.precision$}")
        .trim_start_matches('0')
        .to_owned()
}
fn rate_or_dash(value: f64, precision: usize) -> String {
    if value == 0.0 {
        "—".into()
    } else {
        rate(value, precision)
    }
}
fn games_back(value: &str) -> &str {
    if value.is_empty() || value == "—" || value == "-" {
        "--"
    } else {
        value
    }
}
