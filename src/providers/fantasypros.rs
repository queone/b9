use super::ProviderError;
use crate::transport::{HttpClient, HttpMethod, HttpRequest};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EcrRow {
    pub name: String,
    pub team: String,
    pub yahoo_player_id: Option<i64>,
    pub rank: i64,
}
#[derive(Deserialize)]
struct Envelope {
    players: Vec<Raw>,
}
#[derive(Deserialize)]
struct Raw {
    player_name: String,
    #[serde(default)]
    player_team_id: String,
    #[serde(default)]
    yahoo_player_id: String,
    rank_ecr: i64,
}
pub fn parse_html(body: &str) -> Result<Vec<EcrRow>, ProviderError> {
    let start = body.find("var ecrData").ok_or_else(|| {
        ProviderError::invalid("parse FantasyPros ECR", "ecrData marker is absent")
    })?;
    let open = body[start..].find('{').map(|n| start + n).ok_or_else(|| {
        ProviderError::invalid("parse FantasyPros ECR", "opening object is absent")
    })?;
    let mut depth = 0;
    let mut quoted = false;
    let mut escaped = false;
    let mut end = None;
    for (i, c) in body[open..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && quoted {
            escaped = true;
            continue;
        }
        if c == '"' {
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        if c == '{' {
            depth += 1
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                end = Some(open + i + 1);
                break;
            }
        }
    }
    let env: Envelope = serde_json::from_str(
        &body[open..end.ok_or_else(|| {
            ProviderError::invalid("parse FantasyPros ECR", "closing object is absent")
        })?],
    )
    .map_err(|e| ProviderError::operation("parse FantasyPros ECR", "decode ecrData", e))?;
    Ok(env
        .players
        .into_iter()
        .map(|r| EcrRow {
            name: r.player_name,
            team: r.player_team_id,
            yahoo_player_id: r.yahoo_player_id.parse().ok(),
            rank: r.rank_ecr,
        })
        .collect())
}

pub fn fetch(http: Arc<HttpClient>) -> Result<Vec<EcrRow>, ProviderError> {
    let response = http
        .execute(HttpRequest {
            method: HttpMethod::Get,
            url: "https://www.fantasypros.com/mlb/rankings/overall.php".into(),
            headers: vec![],
            body: vec![],
            timeout: Duration::from_secs(20),
            body_limit: 16 * 1024 * 1024,
        })
        .map_err(|e| ProviderError::operation("fetch FantasyPros ECR", "dispatch request", e))?;
    if !(200..300).contains(&response.status) {
        return Err(ProviderError::invalid(
            "fetch FantasyPros ECR",
            format!("HTTP status {}", response.status),
        ));
    }
    parse_html(
        std::str::from_utf8(&response.body)
            .map_err(|e| ProviderError::operation("parse FantasyPros ECR", "decode UTF-8", e))?,
    )
}
