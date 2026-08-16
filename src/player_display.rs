//! Deterministic roster, player-pool, totals, and detail rendering.

use crate::domain::{MatchupTeam, PlayerGameLog, StoredFantasyPlayer};
use crate::store::{StoredFantasyCategory, StoredFantasyTeam};
use crate::terminal::{HelpColorMode, available, dim, roster_row, table_heading, warning};

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
        for player in rows {
            output.push_str(&player_row(player, roster, mode));
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
    format!(
        "{slot}{player:<26}  {:<5}  {:<17}  {hand:<1}  {:>4}  {pt1:>5}  {pt2:>5}  {s1:>3}  {s2:>3}  {s3:>4}  {s4:>5}  {s5:>5}  OWNER",
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
    let position = fit(&player.positions, 5);
    let status = fit(
        if player.status.is_empty() {
            "NoGame"
        } else {
            &player.status
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
    let row = format!("{slot}{identity}  {position}  {status}  -  {yr:>4}  {stats}  {owner}");
    if player.slot.as_deref() == Some("IL") || player.status.starts_with("IL") {
        warning(&row, mode)
    } else if player.slot.as_deref() == Some("BN") {
        dim(&row, mode)
    } else {
        roster_row(&row, &player.status, mode)
    }
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
    if !stale {
        output.push_str(
            "\nFantasy data provided by Yahoo Fantasy — https://sports.yahoo.com/fantasy/\n",
        );
    }
    output
}

/// Render an available player detail card.
pub fn render_detail(
    player: &StoredFantasyPlayer,
    logs: &[PlayerGameLog],
    stale: bool,
    mode: HelpColorMode,
) -> String {
    let mut output = format!(
        "{}\nPOS {}  TEAM {}  YR {}  OWNER {}\nSEASON  PA {:.0}  OBP {:.3}  R {:.0}  HR {:.0}  RBI {:.0}  SB {:.0}  AVG {:.3}\nSEASON  IP {:.1}  QS {:.0}  W {:.0}  SV {:.0}  K {:.0}  ERA {:.2}  WHIP {:.2}\n",
        table_heading(&format!("{} {}", player.name, player.team), mode),
        player.positions,
        player.team,
        player
            .rank
            .map_or_else(|| "—".into(), |rank| rank.to_string()),
        player.owner.as_deref().unwrap_or("<available>"),
        player.batting[0],
        player.batting[1],
        player.batting[2],
        player.batting[3],
        player.batting[4],
        player.batting[5],
        player.batting[6],
        player.pitching[0],
        player.pitching[1],
        player.pitching[2],
        player.pitching[3],
        player.pitching[4],
        player.pitching[5],
        player.pitching[6]
    );
    if stale {
        output.push_str("GAME LOG data may be stale — refresh unavailable.\n");
    }
    output.push_str("GAME LOG\n");
    if logs.is_empty() {
        output.push_str("No game-log data is available.\n");
    } else {
        for log in logs.iter().rev().take(10).rev() {
            output.push_str(&format!(
                "{}  {:<5}  {}\n",
                log.date, log.opponent, log.line
            ));
        }
    }
    output
}
