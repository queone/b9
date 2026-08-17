//! Deterministic roster, player-pool, totals, and detail rendering.

use crate::domain::{MatchupTeam, PlayerGameLog, StoredFantasyPlayer};
use crate::store::{StoredFantasyCategory, StoredFantasyTeam};
use crate::terminal::{
    HelpColorMode, available, dim, injury_status, roster_row, table_heading, title, warning,
};

const PLAYER_WIDTH: usize = 26;
const STATUS_WIDTH: usize = 17;

/// Render one roster or player-pool table with skout-compatible columns.
pub fn render_players(title: &str, players: &[StoredFantasyPlayer], mode: HelpColorMode) -> String {
    let roster = !matches!(title, "HITTERS" | "PITCHERS");
    let mut output = String::new();
    if roster {
        output.push_str(&format!(
            "{} {}\n",
            table_heading("ROSTER:", mode),
            available(title, mode)
        ));
    }
    for (role, heading) in [("B", "HITTER"), ("P", "PITCHER")] {
        let rows = players
            .iter()
            .filter(|player| player.role == role)
            .collect::<Vec<_>>();
        if rows.is_empty() {
            continue;
        }
        let header = if role == "B" {
            player_header(
                roster, heading, "B", "PA", "OBP", "R", "HR", "RBI", "SB", "AVG",
            )
        } else {
            player_header(
                roster, heading, "T", "IP", "QS", "W", "SV", "K", "ERA", "WHIP",
            )
        };
        output.push_str(&table_heading(&header, mode));
        output.push('\n');
        for player in &rows {
            output.push_str(&player_row(player, roster, mode));
            output.push('\n');
        }
        if roster {
            output.push_str(&roster_total_row(role, &rows, mode));
            output.push('\n');
        }
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn player_header(
    roster: bool,
    player: &str,
    hand: &str,
    pt1: &str,
    pt2: &str,
    s1: &str,
    s2: &str,
    s3: &str,
    s4: &str,
    s5: &str,
) -> String {
    let slot = if roster { "SLOT  " } else { "" };
    let advanced = if roster {
        ""
    } else if player == "HITTER" {
        "  xwOBA     EV   BRL%    HH%     K%    BB%    SPD    OPS"
    } else {
        "    FBV  WHIFF%    CH%    GB%     K%    BB%"
    };
    let owner = if roster { "" } else { "  OWNER" };
    format!(
        "{slot}{player:<26}  {:<5}  {:<17}  {hand:<1}  {:>4}  {pt1:>5}  {pt2:>5}  {s1:>3}  {s2:>3}  {s3:>4}  {s4:>5}  {s5:>5}{advanced}{owner}",
        "POS", "STATUS", "YR"
    )
}

fn player_row(player: &StoredFantasyPlayer, roster: bool, mode: HelpColorMode) -> String {
    let slot = if roster {
        format!("{:<4}  ", player.slot.as_deref().unwrap_or("-"))
    } else {
        String::new()
    };
    let identity = fit(&format!("{} {}", player.name, player.team), PLAYER_WIDTH);
    let position = fit(&display_positions(&player.positions), 5);
    let status = fit(
        if !player.status.is_empty() {
            &player.status
        } else if !player.game_status.is_empty() {
            &player.game_status
        } else {
            "NoGame"
        },
        STATUS_WIDTH,
    );
    let yr = player
        .rank
        .map_or_else(|| "—".into(), |value| value.to_string());
    let owner = match &player.owner {
        Some(owner) => dim(&fit(owner, 20), mode),
        None if player.yahoo_player_id.is_some() => available(&fit("<available>", 20), mode),
        None => dim(&fit("<not yet in Yahoo>", 20), mode),
    };
    let stats = if player.role == "B" {
        format!(
            "{:>5.0}  {:>5}  {:>3.0}  {:>3.0}  {:>4.0}  {:>5.0}  {:>5}",
            player.batting[0],
            rate(player.batting[1], 3),
            player.batting[2],
            player.batting[3],
            player.batting[4],
            player.batting[5],
            rate(player.batting[6], 3),
        )
    } else {
        format!(
            "{:>5.1}  {:>5.0}  {:>3.0}  {:>3.0}  {:>4.0}  {:>5.2}  {:>5.2}",
            player.pitching[0],
            player.pitching[1],
            player.pitching[2],
            player.pitching[3],
            player.pitching[4],
            player.pitching[5],
            player.pitching[6],
        )
    };
    let hand = dim(
        if player.hand.is_empty() {
            "-"
        } else {
            &player.hand
        },
        mode,
    );
    let yr = dim(&format!("{yr:>4}"), mode);
    let stats = if player.role == "B" {
        let (pa, rest) = stats.split_at(5);
        format!("{}{}", dim(pa, mode), dim(&rest[..7.min(rest.len())], mode))
            + &rest[7.min(rest.len())..]
    } else {
        stats
    };
    let advanced = if roster {
        String::new()
    } else {
        advanced_values(player)
    };
    let owner = if roster {
        String::new()
    } else {
        format!("  {owner}")
    };
    let row =
        format!("{slot}{identity}  {position}  {status}  {hand}  {yr}  {stats}{advanced}{owner}");
    if player.slot.as_deref() == Some("IL") || player.status.starts_with("IL") {
        warning(&row, mode)
    } else {
        roster_row(&row, &player.status, mode)
    }
}

fn advanced_values(player: &StoredFantasyPlayer) -> String {
    fn value(value: Option<f64>, width: usize, precision: usize) -> String {
        value.map_or_else(
            || format!("{:>width$}", "—"),
            |value| format!("{value:>width$.precision$}"),
        )
    }
    if player.role == "B" {
        format!(
            "  {}  {}  {}  {}  {}  {}  {}  {}",
            value(player.hitting_advanced[0], 5, 3),
            value(player.hitting_advanced[1], 5, 1),
            value(player.hitting_advanced[2], 5, 1),
            value(player.hitting_advanced[3], 5, 1),
            value(player.hitting_advanced[4], 5, 1),
            value(player.hitting_advanced[5], 5, 1),
            value(player.hitting_advanced[6], 5, 1),
            value(player.hitting_advanced[7], 5, 3)
        )
    } else {
        format!(
            "  {}  {}  {}  {}  {}  {}",
            value(player.pitching_advanced[0], 5, 1),
            value(player.pitching_advanced[1], 6, 1),
            value(player.pitching_advanced[2], 5, 1),
            value(player.pitching_advanced[3], 5, 1),
            value(player.pitching_advanced[4], 5, 1),
            value(player.pitching_advanced[5], 5, 1)
        )
    }
}

fn display_positions(value: &str) -> String {
    let values = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if values.len() <= 1 {
        return value.to_owned();
    }
    values
        .into_iter()
        .filter(|value| !matches!(value.to_ascii_lowercase().as_str(), "uti" | "util"))
        .collect::<Vec<_>>()
        .join(",")
}

fn roster_total_row(role: &str, players: &[&StoredFantasyPlayer], mode: HelpColorMode) -> String {
    let mut batting = [0.0; 7];
    let mut pitching = [0.0; 7];
    for player in players {
        let pa = player.batting[0];
        batting[0] += pa;
        batting[1] += player.batting[1] * pa;
        batting[2] += player.batting[2];
        batting[3] += player.batting[3];
        batting[4] += player.batting[4];
        batting[5] += player.batting[5];
        batting[6] += player.batting[6] * pa;
        let ip = player.pitching[0];
        pitching[0] += ip;
        pitching[1] += player.pitching[1];
        pitching[2] += player.pitching[2];
        pitching[3] += player.pitching[3];
        pitching[4] += player.pitching[4];
        pitching[5] += player.pitching[5] * ip;
        pitching[6] += player.pitching[6] * ip;
    }
    if batting[0] > 0.0 {
        batting[1] /= batting[0];
        batting[6] /= batting[0];
    }
    if pitching[0] > 0.0 {
        pitching[5] /= pitching[0];
        pitching[6] /= pitching[0];
    }
    let stats = if role == "B" {
        format!(
            "{:>5.0}  {:>5}  {:>3.0}  {:>3.0}  {:>4.0}  {:>5.0}  {:>5}",
            batting[0],
            rate(batting[1], 3),
            batting[2],
            batting[3],
            batting[4],
            batting[5],
            rate(batting[6], 3)
        )
    } else {
        format!(
            "{:>5.1}  {:>5.0}  {:>3.0}  {:>3.0}  {:>4.0}  {:>5.2}  {:>5.2}",
            pitching[0],
            pitching[1],
            pitching[2],
            pitching[3],
            pitching[4],
            pitching[5],
            pitching[6]
        )
    };
    title(
        &format!(
            "{:<4}  {:<26}  {:<5}  {:<17}  {:<1}  {:>4}  {stats}",
            "", "TOTAL", "", "", "", ""
        ),
        mode,
    )
}

fn fit(value: &str, width: usize) -> String {
    let mut chars = value.chars().take(width).collect::<String>();
    chars.extend(std::iter::repeat_n(
        ' ',
        width.saturating_sub(chars.chars().count()),
    ));
    chars
}

fn rate(value: f64, precision: usize) -> String {
    let value = format!("{value:.precision$}");
    value.strip_prefix('0').unwrap_or(&value).to_owned()
}

/// Render season totals and standings for every fantasy team.
pub fn render_league_totals(
    teams: &[StoredFantasyTeam],
    players: &[StoredFantasyPlayer],
    mode: HelpColorMode,
) -> String {
    let team_width = teams
        .iter()
        .map(|team| team.name.chars().count())
        .max()
        .unwrap_or(4)
        .max(4);
    let header = format!(
        "{:<team_width$}  {:>4}  {:>8}  {:>5}  {:>5}  {:>5}  {:>5}  {:>4}  {:>5}  {:>5}  {:>5}  {:>3}  {:>3}  {:>3}  {:>3}  {:>5}  {:>6}  {:>3}  {:>3}  {:>3}  {:>4}  {:>5}  {:>5}",
        "TEAM",
        "RANK",
        "WLT",
        "PCT",
        "GB",
        "LW",
        "BDGT",
        "WVR",
        "MOVES",
        "PA",
        "OBP",
        "R",
        "HR",
        "RBI",
        "SB",
        "AVG",
        "IP",
        "QS",
        "W",
        "SV",
        "K",
        "ERA",
        "WHIP"
    );
    let leader = teams
        .iter()
        .max_by_key(|team| team.wins)
        .map(|team| (team.wins, team.losses))
        .unwrap_or((0, 0));
    let mut output = format!("{}\n", table_heading(&header, mode));
    for team in teams {
        let roster = players
            .iter()
            .filter(|player| player.owner.as_deref() == Some(team.name.as_str()))
            .collect::<Vec<_>>();
        let mut batting = [0.0; 7];
        let mut pitching = [0.0; 7];
        let mut weighted_obp = 0.0;
        let mut weighted_avg = 0.0;
        let mut weighted_era = 0.0;
        let mut weighted_whip = 0.0;
        for player in roster {
            let pa = player.batting[0];
            batting[0] += pa;
            batting[2] += player.batting[2];
            batting[3] += player.batting[3];
            batting[4] += player.batting[4];
            batting[5] += player.batting[5];
            weighted_obp += player.batting[1] * pa;
            weighted_avg += player.batting[6] * pa;
            let ip = player.pitching[0];
            pitching[0] += ip;
            pitching[1] += player.pitching[1];
            pitching[2] += player.pitching[2];
            pitching[3] += player.pitching[3];
            pitching[4] += player.pitching[4];
            weighted_era += player.pitching[5] * ip;
            weighted_whip += player.pitching[6] * ip;
        }
        if batting[0] > 0.0 {
            batting[1] = weighted_obp / batting[0];
            batting[6] = weighted_avg / batting[0];
        }
        if pitching[0] > 0.0 {
            pitching[5] = weighted_era / pitching[0];
            pitching[6] = weighted_whip / pitching[0];
        }
        let contextual = format!(
            "  {:>4}  {:>8}  {:>5}  {:>5}  {:>5}  {:>5}  {:>4}  {:>5}  {:>5}  {:>5}",
            nonzero(team.rank),
            wlt(team.wins, team.losses, team.ties),
            percentage(team.wins, team.losses, team.ties),
            games_behind(team.wins, team.losses, leader),
            "—",
            format!("${}", team.faab_balance),
            team.waiver_priority,
            team.moves,
            batting[0] as i64,
            rate_or_dash(batting[1], 3),
        );
        output.push_str(&format!(
            "{:<team_width$}{}",
            team.name,
            dim(&contextual, mode)
        ));
        output.push_str(&format!(
            "  {:>3.0}  {:>3.0}  {:>3.0}  {:>3.0}  {:>5}{}  {:>3.0}  {:>3.0}  {:>4.0}  {:>5.2}  {:>5.2}\n",
            batting[2], batting[3], batting[4], batting[5], rate_or_dash(batting[6], 3),
            dim(&format!("  {:>6.1}  {:>3.0}", pitching[0], pitching[1]), mode),
            pitching[2], pitching[3], pitching[4], pitching[5], pitching[6]
        ));
    }
    output
}

fn nonzero(value: i64) -> String {
    if value > 0 {
        value.to_string()
    } else {
        "—".into()
    }
}

fn wlt(wins: i64, losses: i64, ties: i64) -> String {
    if wins + losses + ties == 0 {
        "—".into()
    } else if ties == 0 {
        format!("{wins}-{losses}")
    } else {
        format!("{wins}-{losses}-{ties}")
    }
}

fn percentage(wins: i64, losses: i64, ties: i64) -> String {
    let total = wins + losses + ties;
    if total == 0 {
        "—".into()
    } else {
        rate((wins as f64 + ties as f64 / 2.0) / total as f64, 3)
    }
}

fn games_behind(wins: i64, losses: i64, leader: (i64, i64)) -> String {
    if wins + losses == 0 {
        return "—".into();
    }
    let value = ((leader.0 - wins + losses - leader.1) as f64) / 2.0;
    if value > 0.0 {
        format!("{value:.1}")
    } else {
        "—".into()
    }
}

fn rate_or_dash(value: f64, precision: usize) -> String {
    if value != 0.0 {
        rate(value, precision)
    } else {
        "—".into()
    }
}

/// Render weekly Yahoo scoring totals in league-defined category order.
pub fn render_weekly_totals(
    title: &str,
    period: &str,
    team: &MatchupTeam,
    categories: &[StoredFantasyCategory],
    stale: bool,
    mode: HelpColorMode,
) -> String {
    let mut output = format!("{}\n{}\n", table_heading(title, mode), period);
    if stale {
        output.push_str("STALE — showing the last complete Yahoo weekly snapshot.\n");
    }
    for category in categories {
        if let Some(value) = team.stats.get(&category.stat_id.to_string()) {
            output.push_str(&format!("{:<6} {}\n", category.abbreviation, value));
        }
    }
    output
}

/// Render an available player detail card.
pub fn render_detail(
    player: &StoredFantasyPlayer,
    logs: &[PlayerGameLog],
    stale: bool,
    today: &str,
    mode: HelpColorMode,
) -> String {
    let role = if player.role == "P" {
        "PITCHER"
    } else {
        "HITTER"
    };
    let rank = player
        .rank
        .map_or_else(|| "—".into(), |rank| rank.to_string());
    let age =
        player_age(&player.birth_date, today).map_or_else(|| "—".into(), |age| age.to_string());
    let summary_header = format!("{role:<22}  POS    STATUS    B  AGE    YR");
    let status = fit(
        if player.status.is_empty() {
            "—"
        } else {
            &player.status
        },
        8,
    );
    let status = if player.status.starts_with("IL") {
        injury_status(&status, mode)
    } else {
        status
    };
    let mut output = format!(
        "{}\n{:<22}  {:<5}  {}  {:<1}  {:>3}  {:>4}\n\n",
        table_heading(&summary_header, mode),
        format!("{} {}", player.name, player.team),
        display_positions(&player.positions),
        status,
        if player.hand.is_empty() {
            "-"
        } else {
            &player.hand
        },
        age,
        rank,
    );
    output.push_str(&detail_source(player, mode));
    output.push('\n');
    output.push_str(&detail_split(player, mode));
    output.push('\n');
    if stale {
        output.push_str("GAME LOG data may be stale — refresh unavailable.\n");
    }
    output.push_str(&table_heading(
        if player.role == "P" {
            "GAME LOG   OPP      STATUS      IP    W   SV    K    ERA   WHIP"
        } else {
            "GAME LOG   OPP      STATUS   H/AB     R    HR   RBI    SB    AVG"
        },
        mode,
    ));
    output.push('\n');
    for date in recent_dates(today) {
        if let Some(log) = logs.iter().find(|log| log.date == date) {
            output.push_str(&detail_log_row(player, &date, log));
        } else {
            output.push_str(&empty_detail_log_row(player, &date));
        }
    }
    if !player.status.is_empty() || !player.injury_note.is_empty() {
        output.push('\n');
        output.push_str(&table_heading("INJURIES", mode));
        output.push('\n');
        if player.injury_note.is_empty() {
            output.push_str(&player.status);
        } else if player.status.is_empty() {
            output.push_str(&player.injury_note);
        } else {
            output.push_str(&format!("{}: {}", player.status, player.injury_note));
        }
        output.push('\n');
    }
    output
}

fn detail_source(player: &StoredFantasyPlayer, mode: HelpColorMode) -> String {
    let owner = player.owner.as_deref().unwrap_or("<available>");
    if player.role == "P" {
        format!(
            "{}\n{:<8} {}  {}  {}  {}  {}  {}  {owner}\n",
            table_heading(
                "SOURCE      FBV  WHIFF%    CH%    GB%     K%    BB%  OWNER",
                mode
            ),
            "SAVANT",
            detail_value(player.pitching_advanced[0], 5, 1),
            detail_percent(player.pitching_advanced[1], 6),
            detail_percent(player.pitching_advanced[2], 5),
            detail_percent(player.pitching_advanced[3], 5),
            detail_percent(player.pitching_advanced[4], 5),
            detail_percent(player.pitching_advanced[5], 5),
        )
    } else {
        format!(
            "{}\n{:<8} {}  {}  {}  {}  {}  {}  {}  {owner}\n",
            table_heading(
                "SOURCE    xwOBA     EV   BRL%    HH%     K%    BB%    SPD  OWNER",
                mode,
            ),
            "SAVANT",
            detail_rate(player.hitting_advanced[0], 5),
            detail_value(player.hitting_advanced[1], 5, 1),
            detail_percent(player.hitting_advanced[2], 5),
            detail_percent(player.hitting_advanced[3], 5),
            detail_percent(player.hitting_advanced[4], 5),
            detail_percent(player.hitting_advanced[5], 5),
            detail_value(player.hitting_advanced[6], 5, 1),
        )
    }
}

fn detail_split(player: &StoredFantasyPlayer, mode: HelpColorMode) -> String {
    if player.role == "P" {
        format!(
            "{}\n{:<8} {:>7.1} {:>4.0} {:>4.0} {:>4.0} {:>5.0} {:>6.2} {:>6.2}\n",
            table_heading("SPLIT         IP   QS    W   SV     K    ERA   WHIP", mode),
            "CURRENT",
            player.pitching[0],
            player.pitching[1],
            player.pitching[2],
            player.pitching[3],
            player.pitching[4],
            player.pitching[5],
            player.pitching[6],
        )
    } else {
        let ops = detail_rate(player.hitting_advanced[7], 5);
        format!(
            "{}\n{:<8} {:>7.0} {:>7} {} {:>4.0} {:>5.0} {:>5.0} {:>4.0} {:>6}\n",
            table_heading(
                "SPLIT           PA     OBP    OPS    R    HR   RBI   SB    AVG",
                mode
            ),
            "CURRENT",
            player.batting[0],
            rate(player.batting[1], 3),
            ops,
            player.batting[2],
            player.batting[3],
            player.batting[4],
            player.batting[5],
            rate(player.batting[6], 3),
        )
    }
}

fn detail_value(value: Option<f64>, width: usize, precision: usize) -> String {
    value.map_or_else(
        || format!("{:>width$}", "—"),
        |value| format!("{value:>width$.precision$}"),
    )
}

fn detail_rate(value: Option<f64>, width: usize) -> String {
    value.map_or_else(
        || format!("{:>width$}", "—"),
        |value| format!("{:>width$}", rate(value, 3)),
    )
}

fn detail_percent(value: Option<f64>, width: usize) -> String {
    value.map_or_else(
        || format!("{:>width$}", "—"),
        |value| format!("{:>width$}", format!("{value:.1}%")),
    )
}

fn detail_log_row(player: &StoredFantasyPlayer, date: &str, log: &PlayerGameLog) -> String {
    if player.role == "P" {
        format!(
            "{:<10} {:<8} {:<8} {:>5} {:>4} {:>4} {:>4} {:>6} {:>6}\n",
            display_date(date),
            log.opponent,
            "",
            log_value(&log.line, "IP"),
            log_value(&log.line, "W"),
            log_value(&log.line, "SV"),
            log_value(&log.line, "K"),
            log_value(&log.line, "ERA"),
            log_value(&log.line, "WHIP"),
        )
    } else {
        let hits = log_value(&log.line, "H");
        let at_bats = log_value(&log.line, "AB");
        let hits_at_bats = if hits == "-" || at_bats == "-" {
            "-".to_owned()
        } else {
            format!("{hits}/{at_bats}")
        };
        format!(
            "{:<10} {:<8} {:<8} {:>5} {:>5} {:>5} {:>5} {:>5} {:>6}\n",
            display_date(date),
            log.opponent,
            "",
            hits_at_bats,
            log_value(&log.line, "R"),
            log_value(&log.line, "HR"),
            log_value(&log.line, "RBI"),
            log_value(&log.line, "SB"),
            normalized_rate(&log_value(&log.line, "AVG")),
        )
    }
}

fn empty_detail_log_row(player: &StoredFantasyPlayer, date: &str) -> String {
    if player.role == "P" {
        format!(
            "{:<10} {:<8} {:<8} {:>5} {:>4} {:>4} {:>4} {:>6} {:>6}\n",
            display_date(date),
            "",
            "",
            "-",
            "-",
            "-",
            "-",
            "-",
            "-"
        )
    } else {
        format!(
            "{:<10} {:<8} {:<8} {:>5} {:>5} {:>5} {:>5} {:>5} {:>6}\n",
            display_date(date),
            "",
            "",
            "-",
            "-",
            "-",
            "-",
            "-",
            "-"
        )
    }
}

fn log_value(line: &str, name: &str) -> String {
    let values = line.split_whitespace().collect::<Vec<_>>();
    values
        .windows(2)
        .find(|pair| pair[0] == name)
        .map_or_else(|| "-".to_owned(), |pair| pair[1].to_owned())
}

fn normalized_rate(value: &str) -> String {
    value
        .parse::<f64>()
        .ok()
        .map_or_else(|| "-".to_owned(), |value| rate(value, 3))
}

fn player_age(birth_date: &str, today: &str) -> Option<i64> {
    let (birth_year, birth_month, birth_day) = parse_ymd(birth_date)?;
    let (year, month, day) = parse_ymd(today)?;
    let age = year - birth_year - i64::from((month, day) < (birth_month, birth_day));
    (age >= 0).then_some(age)
}

fn parse_ymd(value: &str) -> Option<(i64, i64, i64)> {
    let mut parts = value.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    (parts.next().is_none() && (1..=12).contains(&month) && (1..=31).contains(&day))
        .then_some((year, month, day))
}

fn recent_dates(today: &str) -> Vec<String> {
    let Some(days) = parse_date_days(today) else {
        return vec![today.to_owned()];
    };
    (0..10)
        .rev()
        .map(|offset| civil_date(days - offset))
        .collect()
}

fn parse_date_days(value: &str) -> Option<i64> {
    let mut parts = value.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<i64>().ok()?;
    let day = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * adjusted_month + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn display_date(value: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = value
        .get(5..7)
        .and_then(|value| value.parse::<usize>().ok());
    let day = value.get(8..10).unwrap_or(value);
    month
        .and_then(|month| MONTHS.get(month.saturating_sub(1)))
        .map_or_else(|| value.to_owned(), |month| format!("{month} {day}"))
}
