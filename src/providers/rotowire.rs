//! Short-lived acquisition of RotoWire's confirmed daily MLB lineups.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::cache::{CacheLookup, DiskCache};
use crate::providers::ProviderError;
use crate::transport::{HttpClient, HttpMethod, HttpRequest};

const URL: &str = "https://www.rotowire.com/baseball/daily-lineups.php";
const TTL: Duration = Duration::from_secs(2 * 60);
const BODY_LIMIT: usize = 2 * 1024 * 1024;

/// One parsed RotoWire game with ordered lineup names.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DailyLineup {
    pub away_team: String,
    pub home_team: String,
    pub confirmed: bool,
    pub away_players: Vec<String>,
    pub home_players: Vec<String>,
    pub away_pitcher: String,
    pub home_pitcher: String,
}

/// RotoWire lineup acquisition through skout's validated transport.
pub struct RotowireClient {
    http: Arc<HttpClient>,
}

impl RotowireClient {
    /// Construct the production client around the shared HTTP transport.
    pub fn production(http: Arc<HttpClient>) -> Self {
        Self { http }
    }

    /// Return fresh cached lineups or fetch and cache a new page.
    pub fn fetch_cached(&self, cache: &DiskCache) -> Result<Vec<DailyLineup>, ProviderError> {
        if let Ok(CacheLookup::Hit(entry)) = cache.get("rotowire", "daily-lineups", TTL)
            && let Ok(lineups) = serde_json::from_slice(&entry.payload)
        {
            return Ok(lineups);
        }
        let response = self
            .http
            .execute(HttpRequest {
                method: HttpMethod::Get,
                url: URL.into(),
                headers: Vec::new(),
                body: Vec::new(),
                timeout: Duration::from_secs(15),
                body_limit: BODY_LIMIT,
            })
            .map_err(|error| {
                ProviderError::operation("fetch RotoWire lineups", "request daily lineups", error)
            })?;
        if response.status != 200 {
            return Err(ProviderError::invalid(
                "fetch RotoWire lineups",
                format!("HTTP {}", response.status),
            ));
        }
        let page = String::from_utf8(response.body).map_err(|error| {
            ProviderError::operation("fetch RotoWire lineups", "response is not UTF-8", error)
        })?;
        let lineups = parse_daily_lineups(&page);
        if lineups.is_empty() {
            return Err(ProviderError::invalid(
                "fetch RotoWire lineups",
                "response has no lineup boxes",
            ));
        }
        if let Ok(payload) = serde_json::to_vec(&lineups) {
            let _ = cache.put("rotowire", "daily-lineups", &payload);
        }
        Ok(lineups)
    }
}

/// Parse every recognizable daily-lineup box from one RotoWire page.
pub fn parse_daily_lineups(page: &str) -> Vec<DailyLineup> {
    let starts = page
        .match_indices("lineup__box")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    starts
        .iter()
        .enumerate()
        .filter_map(|(offset, start)| {
            let end = starts.get(offset + 1).copied().unwrap_or(page.len());
            let block = &page[*start..end];
            let away_team = team(text_for_class(block, "lineup__mteam is-visit")?);
            let home_team = team(text_for_class(block, "lineup__mteam is-home")?);
            let away = list(block, "lineup__list is-visit");
            let home = list(block, "lineup__list is-home");
            Some(DailyLineup {
                away_team,
                home_team,
                confirmed: block.contains("lineup__status is-confirmed"),
                away_pitcher: highlighted_name(away).unwrap_or_default(),
                home_pitcher: highlighted_name(home).unwrap_or_default(),
                away_players: player_names(away),
                home_players: player_names(home),
            })
        })
        .collect()
}

fn list<'a>(block: &'a str, class: &str) -> &'a str {
    let Some(start) = block.find(class) else {
        return "";
    };
    let tail = &block[start..];
    &tail[..tail.find("</ul>").unwrap_or(tail.len())]
}

fn highlighted_name(list: &str) -> Option<String> {
    let start = list.find("lineup__player-highlight-name")?;
    anchor_text(&list[start..])
}

fn player_names(list: &str) -> Vec<String> {
    list.split("<li")
        .skip(1)
        .filter_map(|row| {
            let class = attribute(row, "class")?;
            if !class
                .split_whitespace()
                .any(|value| value == "lineup__player")
                || class
                    .split_whitespace()
                    .any(|value| value == "lineup__player-highlight")
            {
                return None;
            }
            anchor_text(row)
        })
        .collect()
}

fn text_for_class(block: &str, class: &str) -> Option<String> {
    let start = block.find(class)?;
    let tail = &block[start..];
    let content = &tail[tail.find('>')? + 1..];
    let content = &content[..content.find('<')?];
    Some(decode(content.trim()))
}

fn anchor_text(value: &str) -> Option<String> {
    let tail = &value[value.find("<a")?..];
    let content = &tail[tail.find('>')? + 1..];
    Some(decode(content[..content.find("</a>")?].trim()))
}

fn attribute<'a>(value: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("{name}=\"");
    let tail = &value[value.find(&marker)? + marker.len()..];
    Some(&tail[..tail.find('"')?])
}

fn decode(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn team(value: String) -> String {
    let name = value.split('(').next().unwrap_or(&value).trim();
    match name {
        "Angels" => "LAA",
        "Astros" => "HOU",
        "Athletics" => "ATH",
        "Blue Jays" => "TOR",
        "Braves" => "ATL",
        "Brewers" => "MIL",
        "Cardinals" => "STL",
        "Cubs" => "CHC",
        "D-backs" | "Diamondbacks" => "AZ",
        "Dodgers" => "LAD",
        "Giants" => "SF",
        "Guardians" => "CLE",
        "Mariners" => "SEA",
        "Marlins" => "MIA",
        "Mets" => "NYM",
        "Nationals" => "WSH",
        "Orioles" => "BAL",
        "Padres" => "SD",
        "Phillies" => "PHI",
        "Pirates" => "PIT",
        "Rangers" => "TEX",
        "Rays" => "TB",
        "Red Sox" => "BOS",
        "Reds" => "CIN",
        "Rockies" => "COL",
        "Royals" => "KC",
        "Tigers" => "DET",
        "Twins" => "MIN",
        "White Sox" => "CWS",
        "Yankees" => "NYY",
        other => other,
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_confirmed_teams_pitchers_and_batting_order() {
        let page = r#"<div class="lineup__box"><div class="lineup__mteam is-visit">Yankees (1-0)</div><div class="lineup__mteam is-home">Red Sox</div><ul class="lineup__list is-visit"><li class="lineup__player-highlight"><div class="lineup__player-highlight-name"><a>Gerrit Cole</a></div></li><div class="lineup__status is-confirmed"></div><li class="lineup__player"><div class="lineup__pos">SS</div><a>Anthony Volpe</a></li></ul><ul class="lineup__list is-home"><li class="lineup__player-highlight"><div class="lineup__player-highlight-name"><a>Chris Sale</a></div></li></ul></div>"#;
        let rows = parse_daily_lineups(page);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].away_team, "NYY");
        assert_eq!(rows[0].home_team, "BOS");
        assert!(rows[0].confirmed);
        assert_eq!(rows[0].away_players, vec!["Anthony Volpe".to_owned()]);
        assert_eq!(rows[0].away_pitcher, "Gerrit Cole");
    }
}
