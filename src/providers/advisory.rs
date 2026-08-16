//! Bounded provider-neutral advisory completion adapters.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use crate::advisory::{AdvisoryContext, AdvisoryResponse};
use crate::transport::{HttpClient, HttpHeader, HttpMethod, HttpRequest};

use super::ProviderError;

const REQUEST_LIMIT: usize = 32 * 1024;
const RESPONSE_LIMIT: usize = 64 * 1024;
const MAX_TOKENS: u16 = 600;
const TIMEOUT: Duration = Duration::from_secs(15);

/// A retained advisory-provider API shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdvisoryProvider {
    Gemini,
    Groq,
    Mistral,
    Claude,
    OpenAi,
}

impl AdvisoryProvider {
    /// Parse a configured provider name, including the Groq Llama compatibility label.
    pub fn parse(value: &str) -> Result<Self, ProviderError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "gemini" => Ok(Self::Gemini),
            "groq" | "llama" | "groq/llama" => Ok(Self::Groq),
            "mistral" => Ok(Self::Mistral),
            "claude" | "anthropic" => Ok(Self::Claude),
            "openai" => Ok(Self::OpenAi),
            _ => Err(ProviderError::invalid(
                "select advisory provider",
                "unsupported provider",
            )),
        }
    }

    fn endpoint(self, model: &str) -> String {
        match self {
            Self::Gemini => format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
            ),
            Self::Groq => "https://api.groq.com/openai/v1/chat/completions".into(),
            Self::Mistral => "https://api.mistral.ai/v1/chat/completions".into(),
            Self::Claude => "https://api.anthropic.com/v1/messages".into(),
            Self::OpenAi => "https://api.openai.com/v1/chat/completions".into(),
        }
    }

    /// Return the stable adapter-default model.
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Gemini => "gemini-2.0-flash",
            Self::Groq => "llama-3.3-70b-versatile",
            Self::Mistral => "mistral-small-latest",
            Self::Claude => "claude-3-5-haiku-latest",
            Self::OpenAi => "gpt-4.1-mini",
        }
    }
}

/// A typed, bounded completion client.
pub struct AdvisoryClient {
    http: Arc<HttpClient>,
}

impl AdvisoryClient {
    /// Construct an advisory client over the validated shared transport.
    pub fn new(http: Arc<HttpClient>) -> Self {
        Self { http }
    }

    /// Request one JSON advisory response using an explicitly selected provider.
    pub fn complete(
        &self,
        provider: AdvisoryProvider,
        configured_model: &str,
        credential: &str,
        context: &AdvisoryContext,
    ) -> Result<AdvisoryResponse, ProviderError> {
        if credential.trim().is_empty() {
            return Err(ProviderError::invalid(
                "read advisory credential",
                "credential is empty",
            ));
        }
        let model = if configured_model.trim().is_empty() {
            provider.default_model()
        } else {
            configured_model.trim()
        };
        let prompt = prompt(context)?;
        let (url, headers, body) = request_parts(provider, model, credential, &prompt);
        if body.len() > REQUEST_LIMIT {
            return Err(ProviderError::invalid(
                "build advisory request",
                "request exceeds 32768 bytes",
            ));
        }
        let response = self
            .http
            .execute(HttpRequest {
                method: HttpMethod::Post,
                url,
                headers,
                body,
                timeout: TIMEOUT,
                body_limit: RESPONSE_LIMIT,
            })
            .map_err(|error| {
                ProviderError::operation("request advisory completion", error.to_string(), error)
            })?;
        if !(200..300).contains(&response.status) {
            return Err(ProviderError::invalid(
                "request advisory completion",
                format!("provider returned HTTP {}", response.status),
            ));
        }
        let body: Value = serde_json::from_slice(&response.body).map_err(|_| {
            ProviderError::invalid("parse advisory completion", "response is not JSON")
        })?;
        let text = response_text(provider, &body)?;
        serde_json::from_str(text).map_err(|_| {
            ProviderError::invalid(
                "parse advisory completion",
                "response does not contain advisory JSON",
            )
        })
    }

    /// Discover bounded OpenAI model identifiers suitable for interactive selection.
    pub fn discover_openai_models(&self, credential: &str) -> Result<Vec<String>, ProviderError> {
        if credential.trim().is_empty() {
            return Err(ProviderError::invalid(
                "discover OpenAI models",
                "credential is empty",
            ));
        }
        let response = self
            .http
            .execute(HttpRequest {
                method: HttpMethod::Get,
                url: "https://api.openai.com/v1/models".into(),
                headers: vec![bearer(credential)],
                body: Vec::new(),
                timeout: TIMEOUT,
                body_limit: RESPONSE_LIMIT,
            })
            .map_err(|error| {
                ProviderError::operation("discover OpenAI models", error.to_string(), error)
            })?;
        if response.status != 200 {
            return Err(ProviderError::invalid(
                "discover OpenAI models",
                format!("provider returned HTTP {}", response.status),
            ));
        }
        let value: Value = serde_json::from_slice(&response.body).map_err(|_| {
            ProviderError::invalid("discover OpenAI models", "response is not JSON")
        })?;
        let mut models = value
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|row| row.get("id").and_then(Value::as_str))
            .filter(|id| id.starts_with("gpt-") || id.starts_with("o1") || id.starts_with("o3"))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        models.sort();
        models.dedup();
        models.truncate(100);
        Ok(models)
    }

    /// Validate one credential with a bounded minimal provider request.
    pub fn validate_credential(
        &self,
        provider: AdvisoryProvider,
        configured_model: &str,
        credential: &str,
    ) -> Result<(), ProviderError> {
        let context = AdvisoryContext {
            lineup_candidates: Vec::new(),
            roster_moves: Vec::new(),
        };
        self.complete(provider, configured_model, credential, &context)
            .map(|_| ())
    }
}

fn prompt(context: &AdvisoryContext) -> Result<String, ProviderError> {
    let context = serde_json::to_string(context)
        .map_err(|_| ProviderError::invalid("serialize advisory context", "context is invalid"))?;
    Ok(format!(
        "Return only JSON matching {{\"confirmations\":[string],\"urgent\":[{{\"id\":string,\"summary\":string}}],\"overnight\":[{{\"id\":string,\"summary\":string}}],\"risks\":[string]}}. Recommend only IDs from this context: {context}"
    ))
}

fn request_parts(
    provider: AdvisoryProvider,
    model: &str,
    credential: &str,
    prompt: &str,
) -> (String, Vec<HttpHeader>, Vec<u8>) {
    let mut headers = vec![HttpHeader {
        name: "content-type".into(),
        value: "application/json".into(),
    }];
    let body = match provider {
        AdvisoryProvider::Gemini => {
            headers.push(HttpHeader {
                name: "x-goog-api-key".into(),
                value: credential.into(),
            });
            json!({"contents":[{"parts":[{"text":prompt}]}],"generationConfig":{"maxOutputTokens":MAX_TOKENS,"responseMimeType":"application/json"}})
        }
        AdvisoryProvider::Claude => {
            headers.push(HttpHeader {
                name: "x-api-key".into(),
                value: credential.into(),
            });
            headers.push(HttpHeader {
                name: "anthropic-version".into(),
                value: "2023-06-01".into(),
            });
            json!({"model":model,"max_tokens":MAX_TOKENS,"messages":[{"role":"user","content":prompt}]})
        }
        AdvisoryProvider::OpenAi => {
            headers.push(bearer(credential));
            json!({"model":model,"max_completion_tokens":MAX_TOKENS,"response_format":{"type":"json_object"},"messages":[{"role":"user","content":prompt}]})
        }
        AdvisoryProvider::Groq | AdvisoryProvider::Mistral => {
            headers.push(bearer(credential));
            json!({"model":model,"max_tokens":MAX_TOKENS,"response_format":{"type":"json_object"},"messages":[{"role":"user","content":prompt}]})
        }
    };
    (
        provider.endpoint(model),
        headers,
        serde_json::to_vec(&body).expect("JSON values serialize"),
    )
}

fn bearer(credential: &str) -> HttpHeader {
    HttpHeader {
        name: "authorization".into(),
        value: format!("Bearer {credential}"),
    }
}

fn response_text(provider: AdvisoryProvider, body: &Value) -> Result<&str, ProviderError> {
    let value = match provider {
        AdvisoryProvider::Gemini => body.pointer("/candidates/0/content/parts/0/text"),
        AdvisoryProvider::Claude => body.pointer("/content/0/text"),
        AdvisoryProvider::Groq | AdvisoryProvider::Mistral | AdvisoryProvider::OpenAi => {
            body.pointer("/choices/0/message/content")
        }
    };
    value.and_then(Value::as_str).ok_or_else(|| {
        ProviderError::invalid(
            "parse advisory completion",
            "response is missing generated content",
        )
    })
}

impl fmt::Display for AdvisoryProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Gemini => "gemini",
            Self::Groq => "groq",
            Self::Mistral => "mistral",
            Self::Claude => "claude",
            Self::OpenAi => "openai",
        })
    }
}
