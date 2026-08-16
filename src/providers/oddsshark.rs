//! Bounded OddsShark future MLB moneyline acquisition.

use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ProviderError;
use crate::transport::{HttpClient, HttpHeader, HttpMethod, HttpRequest};

const TIMEOUT: Duration = Duration::from_secs(10);
const BODY_LIMIT: usize = 4 * 1024 * 1024;

/// OddsShark endpoint configuration.
#[derive(Clone, Debug)]
pub struct OddsSharkEndpoints {
    root: Url,
}

impl OddsSharkEndpoints {
    /// Validate an OddsShark endpoint root.
    pub fn new(root: &str) -> Result<Self, ProviderError> {
        let root = Url::parse(root).map_err(|error| {
            ProviderError::operation("configure OddsShark endpoint", "parse endpoint", error)
        })?;
        let loopback = root.scheme() == "http"
            && root
                .host_str()
                .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"));
        if root.scheme() != "https" && !loopback {
            return Err(ProviderError::invalid(
                "configure OddsShark endpoint",
                "endpoint must use HTTPS except for loopback tests",
            ));
        }
        Ok(Self { root })
    }

    /// Return production OddsShark endpoints.
    pub fn production() -> Self {
        Self::new("https://www.oddsshark.com/api/scores/mlb").expect("static endpoint is valid")
    }
}

/// One future-game moneyline pair.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GameLine {
    pub event_id: String,
    pub date: String,
    pub start_time: String,
    pub away_team: String,
    pub home_team: String,
    pub away_moneyline: i64,
    pub home_moneyline: i64,
}

/// Acquires future MLB lines through injected transport.
pub struct OddsSharkClient {
    http: Arc<HttpClient>,
    endpoints: OddsSharkEndpoints,
}

impl OddsSharkClient {
    /// Construct an injected adapter.
    pub fn new(http: Arc<HttpClient>, endpoints: OddsSharkEndpoints) -> Self {
        Self { http, endpoints }
    }

    /// Construct a production adapter.
    pub fn production(http: Arc<HttpClient>) -> Self {
        Self::new(http, OddsSharkEndpoints::production())
    }

    /// Fetch one ISO-date slate.
    pub fn fetch_game_lines(&self, date: &str) -> Result<Vec<GameLine>, ProviderError> {
        if date.len() != 10
            || !date.bytes().enumerate().all(|(index, byte)| {
                matches!(index, 4 | 7) && byte == b'-'
                    || !matches!(index, 4 | 7) && byte.is_ascii_digit()
            })
        {
            return Err(ProviderError::invalid(
                "fetch OddsShark MLB lines",
                "date must use YYYY-MM-DD",
            ));
        }
        let mut url = self.endpoints.root.clone();
        url.query_pairs_mut().append_pair("date", date);
        let response = self
            .http
            .execute(HttpRequest {
                method: HttpMethod::Get,
                url: url.into(),
                headers: vec![HttpHeader {
                    name: "Referer".into(),
                    value: "https://www.oddsshark.com/mlb/scores".into(),
                }],
                body: Vec::new(),
                timeout: TIMEOUT,
                body_limit: BODY_LIMIT,
            })
            .map_err(|error| {
                ProviderError::operation("fetch OddsShark MLB lines", "request failed", error)
            })?;
        if response.status != 200 {
            return Err(ProviderError::invalid(
                "fetch OddsShark MLB lines",
                format!("provider returned HTTP {}", response.status),
            ));
        }
        let value: Value = serde_json::from_slice(&response.body).map_err(|error| {
            ProviderError::operation("fetch OddsShark MLB lines", "decode JSON response", error)
        })?;
        let games = value
            .as_array()
            .or_else(|| value.get("scores").and_then(Value::as_array))
            .or_else(|| value.get("games").and_then(Value::as_array))
            .ok_or_else(|| {
                ProviderError::invalid("fetch OddsShark MLB lines", "game collection is absent")
            })?;
        let mut output = games
            .iter()
            .filter_map(|game| parse_game(game, date))
            .collect::<Vec<_>>();
        output.sort_by(|left, right| {
            (
                left.date.as_str(),
                left.away_team.as_str(),
                left.home_team.as_str(),
                left.event_id.as_str(),
            )
                .cmp(&(
                    right.date.as_str(),
                    right.away_team.as_str(),
                    right.home_team.as_str(),
                    right.event_id.as_str(),
                ))
        });
        Ok(output)
    }
}

fn parse_game(value: &Value, date: &str) -> Option<GameLine> {
    let text = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| value.get(*key).and_then(Value::as_str))
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let number = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| {
                value
                    .get(*key)
                    .and_then(|item| item.as_i64().or_else(|| item.as_str()?.parse().ok()))
            })
            .unwrap_or_default()
    };
    let away_team = text(&["away_team", "awayTeam", "away_name", "awayName"]);
    let home_team = text(&["home_team", "homeTeam", "home_name", "homeName"]);
    let away_moneyline = number(&["away_moneyline", "awayMoneyLine", "away_ml", "awayPrice"]);
    let home_moneyline = number(&["home_moneyline", "homeMoneyLine", "home_ml", "homePrice"]);
    if away_team.is_empty() || home_team.is_empty() || away_moneyline == 0 || home_moneyline == 0 {
        return None;
    }
    let event_id = text(&["id", "event_id", "eventId"]);
    let supplied_date = text(&["date", "game_date", "gameDate"]);
    let start_time = supplied_date.clone();
    Some(GameLine {
        event_id,
        date: supplied_date
            .chars()
            .take(10)
            .collect::<String>()
            .trim()
            .to_owned()
            .pipe_if_empty(date),
        start_time,
        away_team,
        home_team,
        away_moneyline,
        home_moneyline,
    })
}

trait EmptyFallback {
    fn pipe_if_empty(self, fallback: &str) -> String;
}
impl EmptyFallback for String {
    fn pipe_if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.into()
        } else {
            self
        }
    }
}
