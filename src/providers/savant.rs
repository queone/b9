//! Baseball Savant current-season leaderboard acquisition.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;

use super::ProviderError;
use crate::store::StatcastWrite;
use crate::transport::{HttpClient, HttpMethod, HttpRequest};

const TIMEOUT: Duration = Duration::from_secs(20);
const BODY_LIMIT: usize = 16 * 1024 * 1024;
const BATTING_HEADERS: &[&[&str]] = &[
    &["player_id", "playerid", "id"],
    &["est_woba", "xwoba"],
    &["exit_velocity_avg", "avg_hit_speed", "ev"],
    &["brl_percent", "barrel_batted_rate", "barrel_pct"],
    &["hard_hit_percent", "hard_hit_pct"],
    &["k_percent", "strikeout_percent", "strikeout_pct"],
    &["bb_percent", "walk_percent", "walk_pct"],
    &["sprint_speed"],
    &["on_base_plus_slg", "ops"],
];
const PITCHING_HEADERS: &[&[&str]] = &[
    &["player_id", "playerid", "id"],
    &["ff_avg_speed", "fastball_velo", "fbv"],
    &["whiff_percent", "whiff_pct"],
    &["chase_percent", "oz_swing_percent", "ch_pct"],
    &["groundballs_percent", "gb_percent", "gb_pct"],
    &["k_percent", "strikeout_percent", "strikeout_pct"],
    &["bb_percent", "walk_percent", "walk_pct"],
];

/// Baseball Savant endpoint configuration.
#[derive(Clone, Debug)]
pub struct SavantEndpoints {
    root: Url,
}

impl SavantEndpoints {
    /// Construct one validated endpoint root.
    pub fn new(root: &str) -> Result<Self, ProviderError> {
        let root = Url::parse(root).map_err(|error| {
            ProviderError::operation(
                "configure Baseball Savant endpoint",
                "parse endpoint",
                error,
            )
        })?;
        let loopback = root.scheme() == "http"
            && root
                .host_str()
                .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"));
        if root.scheme() != "https" && !loopback {
            return Err(ProviderError::invalid(
                "configure Baseball Savant endpoint",
                "endpoint must use HTTPS except for loopback tests",
            ));
        }
        Ok(Self { root })
    }

    /// Return the production leaderboard endpoint.
    pub fn production() -> Self {
        Self::new("https://baseballsavant.mlb.com/leaderboard/custom")
            .expect("static Savant endpoint is valid")
    }
}

/// Baseball Savant provider using the shared validated transport.
pub struct SavantClient {
    http: Arc<HttpClient>,
    endpoints: SavantEndpoints,
}

impl SavantClient {
    /// Construct an injected Savant client.
    pub fn new(http: Arc<HttpClient>, endpoints: SavantEndpoints) -> Self {
        Self { http, endpoints }
    }

    /// Construct the production Savant client.
    pub fn production(http: Arc<HttpClient>) -> Self {
        Self::new(http, SavantEndpoints::production())
    }

    /// Fetch and parse one current-season batting snapshot.
    pub fn fetch_batting(&self, season: i64) -> Result<Vec<StatcastWrite>, ProviderError> {
        self.fetch(season, "batter", "batting")
    }

    /// Fetch and parse one current-season pitching snapshot.
    pub fn fetch_pitching(&self, season: i64) -> Result<Vec<StatcastWrite>, ProviderError> {
        self.fetch(season, "pitcher", "pitching")
    }

    fn fetch(
        &self,
        season: i64,
        kind: &str,
        group: &str,
    ) -> Result<Vec<StatcastWrite>, ProviderError> {
        if !(2000..=2200).contains(&season) {
            return Err(ProviderError::invalid(
                "fetch Baseball Savant leaderboard",
                "season is outside the supported range",
            ));
        }
        let mut url = self.endpoints.root.clone();
        let selections = if group == "batting" {
            "xwoba,exit_velocity_avg,barrel_batted_rate,hard_hit_percent,k_percent,bb_percent,sprint_speed,on_base_plus_slg"
        } else {
            "ff_avg_speed,whiff_percent,oz_swing_percent,groundballs_percent,k_percent,bb_percent"
        };
        url.query_pairs_mut()
            .append_pair("year", &season.to_string())
            .append_pair("type", kind)
            .append_pair("filter", "")
            .append_pair("sort", "4")
            .append_pair("sortDir", "desc")
            .append_pair("min", "1")
            .append_pair("selections", selections)
            .append_pair("csv", "true");
        let response = self
            .http
            .execute(HttpRequest {
                method: HttpMethod::Get,
                url: url.into(),
                headers: Vec::new(),
                body: Vec::new(),
                timeout: TIMEOUT,
                body_limit: BODY_LIMIT,
            })
            .map_err(|error| {
                ProviderError::operation(
                    "fetch Baseball Savant leaderboard",
                    "dispatch request",
                    error,
                )
            })?;
        if !(200..300).contains(&response.status) {
            return Err(ProviderError::invalid(
                "fetch Baseball Savant leaderboard",
                format!("HTTP status {}", response.status),
            ));
        }
        parse_csv(&response.body, season, group)
    }
}

/// Parse one complete Savant CSV response.
pub fn parse_csv(
    bytes: &[u8],
    season: i64,
    group: &str,
) -> Result<Vec<StatcastWrite>, ProviderError> {
    if !matches!(group, "batting" | "pitching") {
        return Err(ProviderError::invalid(
            "parse Baseball Savant leaderboard",
            "stat group must be batting or pitching",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|error| {
        ProviderError::operation(
            "parse Baseball Savant leaderboard",
            "response is not UTF-8",
            error,
        )
    })?;
    let mut lines = text.lines();
    let headers = csv_line(lines.next().unwrap_or_default())
        .into_iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if headers.is_empty() {
        return Err(ProviderError::invalid(
            "parse Baseball Savant leaderboard",
            "CSV header is absent",
        ));
    }
    let required = if group == "batting" {
        BATTING_HEADERS
    } else {
        PITCHING_HEADERS
    };
    for aliases in required {
        if !aliases
            .iter()
            .any(|alias| headers.iter().any(|header| header == alias))
        {
            return Err(ProviderError::invalid(
                "parse Baseball Savant leaderboard",
                format!("CSV lacks required column {}", aliases[0]),
            ));
        }
    }
    let mut rows = Vec::new();
    for (offset, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let values = csv_line(line);
        if values.len() != headers.len() {
            return Err(ProviderError::invalid(
                "parse Baseball Savant leaderboard",
                format!(
                    "row {} has {} fields; expected {}",
                    offset + 2,
                    values.len(),
                    headers.len()
                ),
            ));
        }
        let map = headers
            .iter()
            .cloned()
            .zip(values)
            .collect::<BTreeMap<_, _>>();
        let mlbam_id = integer(&map, &["player_id", "playerid", "id"]);
        if mlbam_id <= 0 {
            return Err(ProviderError::invalid(
                "parse Baseball Savant leaderboard",
                format!("row {} lacks a positive player id", offset + 2),
            ));
        }
        rows.push(StatcastWrite {
            mlbam_id,
            season,
            stat_group: group.into(),
            xwoba: number(&map, &["est_woba", "xwoba"]),
            exit_velo_avg: number(&map, &["exit_velocity_avg", "avg_hit_speed", "ev"]),
            barrel_pct: number(&map, &["brl_percent", "barrel_batted_rate", "barrel_pct"]),
            hard_hit_pct: number(&map, &["hard_hit_percent", "hard_hit_pct"]),
            sprint_speed: number(&map, &["sprint_speed"]),
            strikeout_pct: number(&map, &["k_percent", "strikeout_percent", "strikeout_pct"]),
            walk_pct: number(&map, &["bb_percent", "walk_percent", "walk_pct"]),
            ops: number(&map, &["on_base_plus_slg", "ops"]),
            fastball_velo: number(&map, &["ff_avg_speed", "fastball_velo", "fbv"]),
            whiff_pct: number(&map, &["whiff_percent", "whiff_pct"]),
            chase_pct: number(&map, &["chase_percent", "oz_swing_percent", "ch_pct"]),
            gb_pct: number(&map, &["groundballs_percent", "gb_percent", "gb_pct"]),
        });
    }
    if rows.is_empty() {
        return Err(ProviderError::invalid(
            "parse Baseball Savant leaderboard",
            "CSV contains no player rows",
        ));
    }
    Ok(rows)
}

fn integer(map: &BTreeMap<String, String>, names: &[&str]) -> i64 {
    names
        .iter()
        .find_map(|name| map.get(*name))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}
fn number(map: &BTreeMap<String, String>, names: &[&str]) -> Option<f64> {
    names
        .iter()
        .find_map(|name| map.get(*name))
        .and_then(|value| value.trim().trim_end_matches('%').parse().ok())
}

fn csv_line(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut value = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                value.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                values.push(value.trim().to_owned());
                value.clear();
            }
            _ => value.push(character),
        }
    }
    values.push(value.trim().to_owned());
    values
}
