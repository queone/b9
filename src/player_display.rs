//! Deterministic roster, player-pool, and detail rendering.

use crate::domain::{MatchupTeam, PlayerGameLog, StoredFantasyPlayer};
use crate::store::StoredFantasyCategory;
use crate::terminal::{HelpColorMode, section};

/// Render one roster or player-pool table.
pub fn render_players(title: &str, players: &[StoredFantasyPlayer], mode: HelpColorMode) -> String {
    let mut output = format!("{}\n", section(title, mode));
    output.push_str("PLAYER                    POS    TEAM   YR  OWNER\n");
    for player in players {
        let owner = player.owner.as_deref().unwrap_or("<available>");
        output.push_str(&format!(
            "{:<25}  {:<5}  {:<4}  {:>3}  {}\n",
            format!("{} {}", player.name, player.team),
            player.positions,
            player.team,
            player
                .rank
                .map_or_else(|| "—".into(), |rank| rank.to_string()),
            owner
        ));
    }
    output
}

/// Render season totals for a fantasy team.
pub fn render_totals(
    title: &str,
    players: &[StoredFantasyPlayer],
    weekly: Option<&str>,
    mode: HelpColorMode,
) -> String {
    let batting = players.iter().fold([0.0; 7], |mut total, player| {
        for (index, value) in player.batting.iter().enumerate() {
            total[index] += value;
        }
        total
    });
    let pitching = players.iter().fold([0.0; 7], |mut total, player| {
        for (index, value) in player.pitching.iter().enumerate() {
            total[index] += value;
        }
        total
    });
    let period = weekly.map_or("SEASON", |value| {
        if value == "true" {
            "CURRENT WEEK"
        } else {
            value
        }
    });
    format!(
        "{}\n{}\nHITTERS  PA {:>5.0}  OBP {:>.3}  R {:>3.0}  HR {:>3.0}  RBI {:>3.0}  SB {:>3.0}  AVG {:>.3}\nPITCHERS IP {:>5.1}  QS {:>3.0}  W {:>3.0}  SV {:>3.0}  K {:>3.0}  ERA {:>.2}  WHIP {:>.2}\n",
        section(title, mode),
        period,
        batting[0],
        batting[1],
        batting[2],
        batting[3],
        batting[4],
        batting[5],
        batting[6],
        pitching[0],
        pitching[1],
        pitching[2],
        pitching[3],
        pitching[4],
        pitching[5],
        pitching[6]
    )
}

/// Render weekly Yahoo scoring totals in the league-defined category order.
pub fn render_weekly_totals(
    title: &str,
    period: &str,
    team: &MatchupTeam,
    categories: &[StoredFantasyCategory],
    stale: bool,
    mode: HelpColorMode,
) -> String {
    let mut output = format!("{}\n{}\n", section(title, mode), period);
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
        section(&format!("{} {}", player.name, player.team), mode),
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
