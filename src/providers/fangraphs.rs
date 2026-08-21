use super::ProviderError;
use crate::transport::{HttpClient, HttpHeader, HttpMethod, HttpRequest};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct LeaderRow {
    #[serde(rename = "playerid")]
    #[serde(deserialize_with = "deserialize_id")]
    pub fangraphs_id: String,
    #[serde(rename = "xMLBAMID")]
    pub mlbam_id: Option<i64>,
    #[serde(rename = "FB%", default)]
    pub fb_pct: f64,
    #[serde(rename = "HR/FB", default)]
    pub hr_fb_pct: f64,
}
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ProjectionRow {
    #[serde(rename = "playerid")]
    #[serde(deserialize_with = "deserialize_id")]
    pub fangraphs_id: String,
    #[serde(rename = "xMLBAMID")]
    pub mlbam_id: Option<i64>,
    #[serde(rename = "PA", default)]
    pub pa: f64,
    #[serde(rename = "IP", default)]
    pub ip: f64,
    #[serde(rename = "HR", default)]
    pub hr: f64,
    #[serde(rename = "R", default)]
    pub r: f64,
    #[serde(rename = "RBI", default)]
    pub rbi: f64,
    #[serde(rename = "SB", default)]
    pub sb: f64,
    #[serde(rename = "AVG", default)]
    pub avg: f64,
    #[serde(rename = "OBP", default)]
    pub obp: f64,
    #[serde(rename = "SLG", default)]
    pub slg: f64,
    #[serde(rename = "ERA", default)]
    pub era: f64,
    #[serde(rename = "WHIP", default)]
    pub whip: f64,
    #[serde(rename = "SO", default)]
    pub k: f64,
    #[serde(rename = "W", default)]
    pub w: f64,
    #[serde(rename = "SV", default)]
    pub sv: f64,
    #[serde(rename = "BB", default)]
    pub bb: f64,
}

/// Normalize a `playerid` into a string regardless of wire shape: the
/// leaderboard endpoint emits a bare JSON integer, the projections endpoint
/// mostly emits an alphanumeric string (e.g. `"sa3020134"`) that a plain
/// integer parse would reject.
fn deserialize_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        Number(i64),
        Text(String),
    }
    Ok(match Id::deserialize(deserializer)? {
        Id::Number(value) => value.to_string(),
        Id::Text(value) => value,
    })
}

/// Resolve one projection row's MLBAM id: prefer the row's own `xMLBAMID`,
/// falling back to a leaderboard-built crosswalk keyed by `fangraphs_id`.
pub fn resolve_mlbam_id(
    own_id: Option<i64>,
    fangraphs_id: &str,
    crosswalk: &BTreeMap<String, i64>,
) -> Option<i64> {
    own_id.or_else(|| crosswalk.get(fangraphs_id).copied())
}

pub struct FangraphsClient {
    http: Arc<HttpClient>,
}
impl FangraphsClient {
    pub fn new(http: Arc<HttpClient>) -> Self {
        Self { http }
    }
    pub fn fetch_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<Vec<T>, ProviderError> {
        let response = self
            .http
            .execute(HttpRequest {
                method: HttpMethod::Get,
                url: url.into(),
                headers: vec![HttpHeader {
                    name: "Accept".into(),
                    value: "application/json".into(),
                }],
                body: vec![],
                timeout: Duration::from_secs(20),
                body_limit: 16 * 1024 * 1024,
            })
            .map_err(|e| ProviderError::operation("fetch FanGraphs data", "dispatch request", e))?;
        if !(200..300).contains(&response.status) {
            return Err(ProviderError::invalid(
                "fetch FanGraphs data",
                format!("HTTP status {}", response.status),
            ));
        }
        let value: serde_json::Value = serde_json::from_slice(&response.body)
            .map_err(|e| ProviderError::operation("parse FanGraphs JSON", "decode response", e))?;
        serde_json::from_value(value.get("data").cloned().unwrap_or(value))
            .map_err(|e| ProviderError::operation("parse FanGraphs JSON", "decode rows", e))
    }
}
