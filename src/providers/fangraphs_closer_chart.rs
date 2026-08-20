use super::ProviderError;
use crate::transport::{HttpClient, HttpHeader, HttpMethod, HttpRequest};
use std::sync::Arc;
use std::time::Duration;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloserChartEntry {
    pub team: String,
    pub name: String,
    pub role: String,
}
pub fn parse_html(body: &str) -> Result<Vec<CloserChartEntry>, ProviderError> {
    let mut out = Vec::new();
    for row in body.split("<tr").skip(1) {
        let team = cell(row, "TEAM");
        let name = cell(row, "PLAYER");
        let role = cell(row, "PROJECTED ROLE");
        if let (Some(team), Some(name), Some(role)) = (team, name, role) {
            out.push(CloserChartEntry {
                team: normalize_team(&team),
                name,
                role,
            });
        }
    }
    if out.is_empty() {
        return Err(ProviderError::invalid(
            "parse FanGraphs closer chart",
            "no complete closer rows were found",
        ));
    }
    Ok(out)
}
fn cell(row: &str, stat: &str) -> Option<String> {
    let marker = format!("data-stat=\"{stat}\"");
    let tail = row.split_once(&marker)?.1;
    let text = tail.split_once('>')?.1.split_once("</td>")?.0;
    Some(strip_tags(text).trim().to_owned()).filter(|v| !v.is_empty())
}
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in s.chars() {
        match c {
            '<' => tag = true,
            '>' => tag = false,
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    out
}
fn normalize_team(v: &str) -> String {
    match v {
        "WSN" => "WSH",
        "SFG" => "SF",
        "CHW" => "CWS",
        "SDP" => "SD",
        "TBR" => "TB",
        "KCR" => "KC",
        x => x,
    }
    .into()
}

pub fn fetch(http: Arc<HttpClient>) -> Result<Vec<CloserChartEntry>, ProviderError> {
    let response = http
        .execute(HttpRequest {
            method: HttpMethod::Get,
            url: "https://www.fangraphs.com/roster-resource/closer-depth-chart".into(),
            headers: vec![
                HttpHeader {
                    name: "User-Agent".into(),
                    value: "Mozilla/5.0 (compatible; skout) AppleWebKit/537.36".into(),
                },
                HttpHeader {
                    name: "Accept".into(),
                    value: "text/html,application/xhtml+xml".into(),
                },
            ],
            body: vec![],
            timeout: Duration::from_secs(20),
            body_limit: 16 * 1024 * 1024,
        })
        .map_err(|e| {
            ProviderError::operation("fetch FanGraphs closer chart", "dispatch request", e)
        })?;
    if !(200..300).contains(&response.status) {
        return Err(ProviderError::invalid(
            "fetch FanGraphs closer chart",
            format!("HTTP status {}", response.status),
        ));
    }
    parse_html(
        std::str::from_utf8(&response.body).map_err(|e| {
            ProviderError::operation("parse FanGraphs closer chart", "decode UTF-8", e)
        })?,
    )
}
