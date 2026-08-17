//! ESPN MLB scoreboard and moneyline acquisition.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::Url;
use serde::{Deserialize, Serialize};

use super::ProviderError;
use crate::transport::{HttpClient, HttpHeader, HttpMethod, HttpRequest};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const RESPONSE_BODY_LIMIT: usize = 4 * 1024 * 1024;
const MAX_ISSUE_DETAIL: usize = 256;
const JSON_ACCEPT: &str = "application/json";
const CLIENT_CONTACT: &str = "https://github.com/queone/b9";

/// Production ESPN endpoint configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EspnEndpoints {
    scoreboard: Url,
    odds: Url,
}

impl EspnEndpoints {
    /// Construct validated ESPN endpoint roots.
    pub fn new(scoreboard: &str, odds: &str) -> Result<Self, ProviderError> {
        Ok(Self {
            scoreboard: validate_endpoint("scoreboard", scoreboard)?,
            odds: validate_endpoint("odds", odds)?,
        })
    }

    /// Return the production ESPN endpoint roots.
    pub fn production() -> Self {
        Self::new(
            "https://site.api.espn.com/apis/site/v2/sports/baseball/mlb/scoreboard",
            "https://sports.core.api.espn.com/v2/sports/baseball/leagues/mlb/",
        )
        .expect("static ESPN endpoints are valid")
    }
}

/// One normalized ESPN game and its optional top-provider moneyline.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GameLine {
    pub event_id: String,
    pub competition_id: String,
    pub home_team: String,
    pub away_team: String,
    pub sportsbook: String,
    pub home_moneyline: i64,
    pub away_moneyline: i64,
    pub quoted: bool,
}

/// One degraded per-game odds fetch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OddsIssue {
    pub event_id: String,
    pub detail: String,
}

/// One complete two-day ESPN acquisition result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SlateLines {
    pub games: Vec<GameLine>,
    pub issues: Vec<OddsIssue>,
}

/// Compare an ESPN team name with an MLB team name through punctuation-insensitive folding.
pub fn matches_team(left: &str, right: &str) -> bool {
    let fold = |value: &str| {
        value
            .chars()
            .filter(|character| character.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    fold(left) == fold(right)
}

/// Acquires ESPN JSON through an injected validating HTTP client.
pub struct EspnClient {
    http: Arc<HttpClient>,
    endpoints: EspnEndpoints,
}

impl EspnClient {
    /// Construct an ESPN adapter with injected transport and endpoints.
    pub fn new(http: Arc<HttpClient>, endpoints: EspnEndpoints) -> Self {
        Self { http, endpoints }
    }

    /// Construct an ESPN adapter with production endpoints.
    pub fn production(http: Arc<HttpClient>) -> Self {
        Self::new(http, EspnEndpoints::production())
    }

    /// Fetch the supplied UTC day and following UTC day with per-game odds.
    pub fn fetch_game_lines(&self, day: SystemTime) -> Result<SlateLines, ProviderError> {
        let first_day = utc_date(day)?;
        let second_day = next_date(first_day)?;
        let mut seen = HashSet::new();
        let mut events = Vec::new();
        for date in [first_day, second_day] {
            let response: ScoreboardResponse = self.get_json(
                "fetch ESPN scoreboard",
                scoreboard_url(&self.endpoints.scoreboard, date),
            )?;
            for event in response.events {
                if event.id.trim().is_empty() || !seen.insert(event.id.clone()) {
                    continue;
                }
                let Some(competition) = event.competitions.into_iter().next() else {
                    continue;
                };
                if competition.id.trim().is_empty() {
                    continue;
                }
                let mut home = String::new();
                let mut away = String::new();
                for competitor in competition.competitors {
                    match competitor.home_away.as_str() {
                        "home" => home = competitor.team.display_name,
                        "away" => away = competitor.team.display_name,
                        _ => {}
                    }
                }
                if home.trim().is_empty() || away.trim().is_empty() {
                    continue;
                }
                events.push((event.id, competition.id, home, away));
            }
        }

        let mut games = Vec::with_capacity(events.len());
        let mut issues = Vec::new();
        for (event_id, competition_id, home_team, away_team) in events {
            let mut line = GameLine {
                event_id: event_id.clone(),
                competition_id: competition_id.clone(),
                home_team,
                away_team,
                sportsbook: String::new(),
                home_moneyline: 0,
                away_moneyline: 0,
                quoted: false,
            };
            let url = odds_url(&self.endpoints.odds, &event_id, &competition_id);
            match self.get_json::<OddsResponse>("fetch ESPN odds", url) {
                Ok(response) => {
                    if let Some(item) = response.items.into_iter().next() {
                        line.sportsbook = item.provider.name;
                        line.home_moneyline = item.home_team_odds.money_line;
                        line.away_moneyline = item.away_team_odds.money_line;
                        line.quoted = line.home_moneyline != 0 || line.away_moneyline != 0;
                    }
                }
                Err(error) => issues.push(OddsIssue {
                    event_id: event_id.clone(),
                    detail: bounded(&error.to_string(), MAX_ISSUE_DETAIL),
                }),
            }
            games.push(line);
        }
        Ok(SlateLines { games, issues })
    }

    fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        operation: &'static str,
        url: Url,
    ) -> Result<T, ProviderError> {
        let response = self
            .http
            .execute(HttpRequest {
                method: HttpMethod::Get,
                url: url.into(),
                headers: vec![
                    HttpHeader {
                        name: "User-Agent".into(),
                        value: format!("b9/{} (+{CLIENT_CONTACT})", env!("CARGO_PKG_VERSION")),
                    },
                    HttpHeader {
                        name: "Accept".into(),
                        value: JSON_ACCEPT.into(),
                    },
                ],
                body: Vec::new(),
                timeout: REQUEST_TIMEOUT,
                body_limit: RESPONSE_BODY_LIMIT,
            })
            .map_err(|error| ProviderError::operation(operation, "request failed", error))?;
        if response.status != 200 {
            return Err(ProviderError::invalid(
                operation,
                format!("provider returned HTTP {}", response.status),
            ));
        }
        serde_json::from_slice(&response.body)
            .map_err(|error| ProviderError::operation(operation, "decode JSON response", error))
    }
}

#[derive(Deserialize)]
struct ScoreboardResponse {
    #[serde(default)]
    events: Vec<ScoreboardEvent>,
}

#[derive(Deserialize)]
struct ScoreboardEvent {
    #[serde(default)]
    id: String,
    #[serde(default)]
    competitions: Vec<Competition>,
}

#[derive(Deserialize)]
struct Competition {
    #[serde(default)]
    id: String,
    #[serde(default)]
    competitors: Vec<Competitor>,
}

#[derive(Deserialize)]
struct Competitor {
    #[serde(default, rename = "homeAway")]
    home_away: String,
    #[serde(default)]
    team: Team,
}

#[derive(Default, Deserialize)]
struct Team {
    #[serde(default, rename = "displayName")]
    display_name: String,
}

#[derive(Deserialize)]
struct OddsResponse {
    #[serde(default)]
    items: Vec<OddsItem>,
}

#[derive(Deserialize)]
struct OddsItem {
    #[serde(default)]
    provider: Provider,
    #[serde(default, rename = "homeTeamOdds")]
    home_team_odds: TeamOdds,
    #[serde(default, rename = "awayTeamOdds")]
    away_team_odds: TeamOdds,
}

#[derive(Default, Deserialize)]
struct Provider {
    #[serde(default)]
    name: String,
}

#[derive(Default, Deserialize)]
struct TeamOdds {
    #[serde(default, rename = "moneyLine")]
    money_line: i64,
}

fn validate_endpoint(label: &str, value: &str) -> Result<Url, ProviderError> {
    let url = Url::parse(value).map_err(|error| {
        ProviderError::operation(
            "configure ESPN endpoints",
            format!("{label} URL is invalid"),
            error,
        )
    })?;
    let loopback_http = url.scheme() == "http"
        && url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
    if url.scheme() != "https" && !loopback_http {
        return Err(ProviderError::invalid(
            "configure ESPN endpoints",
            format!("{label} URL must use HTTPS or loopback HTTP"),
        ));
    }
    if url.username() != ""
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
    {
        return Err(ProviderError::invalid(
            "configure ESPN endpoints",
            format!("{label} URL must not contain credentials, a query, or a fragment"),
        ));
    }
    Ok(url)
}

fn scoreboard_url(base: &Url, date: (i32, u32, u32)) -> Url {
    let mut url = base.clone();
    url.query_pairs_mut()
        .clear()
        .append_pair("dates", &format!("{:04}{:02}{:02}", date.0, date.1, date.2));
    url
}

fn odds_url(base: &Url, event_id: &str, competition_id: &str) -> Url {
    let mut url = base.clone();
    {
        let mut segments = url.path_segments_mut().expect("validated hierarchical URL");
        segments.pop_if_empty();
        segments.extend(["events", event_id, "competitions", competition_id, "odds"]);
    }
    url
}

fn utc_date(day: SystemTime) -> Result<(i32, u32, u32), ProviderError> {
    let seconds = day.duration_since(UNIX_EPOCH).map_err(|_| {
        ProviderError::invalid(
            "fetch ESPN game lines",
            "supplied day precedes the Unix epoch",
        )
    })?;
    civil_from_days((seconds.as_secs() / 86_400) as i64)
}

fn next_date(date: (i32, u32, u32)) -> Result<(i32, u32, u32), ProviderError> {
    let days = days_from_civil(date.0, date.1, date.2)
        .checked_add(1)
        .ok_or_else(|| {
            ProviderError::invalid("fetch ESPN game lines", "supplied day is too large")
        })?;
    civil_from_days(days)
}

fn civil_from_days(days: i64) -> Result<(i32, u32, u32), ProviderError> {
    let z = days.checked_add(719_468).ok_or_else(|| {
        ProviderError::invalid("fetch ESPN game lines", "supplied day is too large")
    })?;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(m <= 2);
    let year = i32::try_from(year).map_err(|_| {
        ProviderError::invalid(
            "fetch ESPN game lines",
            "supplied day is outside the date range",
        )
    })?;
    Ok((year, m as u32, d as u32))
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = i64::from(month);
    let day = i64::from(day);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn bounded(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}
