use crate::transport::{HttpClient, HttpHeader, HttpMethod, HttpRequest};
use std::time::Duration;
pub fn run(host: &str, path: &str) -> Result<String, String> {
    let client = HttpClient::production().map_err(|e| format!("fetch: {e}"))?;
    run_with_client(&client, host, path)
}

/// Execute a provider-origin-pinned debug request through an injected client.
pub fn run_with_client(client: &HttpClient, host: &str, path: &str) -> Result<String, String> {
    if path.contains("://") || path.starts_with("//") {
        return Err(
            "fetch: path must remain on the selected provider origin; correct the path and retry"
                .into(),
        );
    }
    let (base, headers) = match host {
        "mlb" => ("https://statsapi.mlb.com", vec![]),
        "espn" => (
            "https://site.api.espn.com",
            vec![
                HttpHeader {
                    name: "User-Agent".into(),
                    value: format!(
                        "b9/{} (+https://github.com/queone/b9)",
                        env!("CARGO_PKG_VERSION")
                    ),
                },
                HttpHeader {
                    name: "Accept".into(),
                    value: "application/json".into(),
                },
            ],
        ),
        "oddsshark" => (
            "https://www.oddsshark.com",
            vec![HttpHeader {
                name: "Referer".into(),
                value: "https://www.oddsshark.com/mlb/scores".into(),
            }],
        ),
        "rotowire" => ("https://www.rotowire.com", vec![]),
        "savant" => ("https://baseballsavant.mlb.com", vec![]),
        "yahoo" => (
            "https://pub-api-ro.fantasysports.yahoo.com",
            vec![HttpHeader {
                name: "Accept".into(),
                value: "application/json".into(),
            }],
        ),
        "fangraphs" => (
            "https://www.fangraphs.com",
            vec![
                HttpHeader {
                    name: "User-Agent".into(),
                    value: "Mozilla/5.0 (compatible; b9) AppleWebKit/537.36".into(),
                },
                HttpHeader {
                    name: "Accept".into(),
                    value: "text/html,application/xhtml+xml".into(),
                },
            ],
        ),
        "fantasypros" => ("https://www.fantasypros.com", vec![]),
        _ => {
            return Err(format!(
                "fetch: unknown host {host}; choose a documented provider and retry"
            ));
        }
    };
    let url = format!("{base}/{}", path.trim_start_matches('/'));
    let response = client
        .execute(HttpRequest {
            method: HttpMethod::Get,
            url,
            headers,
            body: vec![],
            timeout: Duration::from_secs(20),
            body_limit: 16 * 1024 * 1024,
        })
        .map_err(|e| format!("fetch: {e}"))?;
    let mut out = format!("HTTP {}\n", response.status);
    for h in response.headers {
        out.push_str(&format!("{}: {}\n", h.name, h.value));
    }
    out.push('\n');
    out.push_str(&String::from_utf8_lossy(&response.body));
    Ok(out)
}
