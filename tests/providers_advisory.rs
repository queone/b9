use std::sync::{Arc, Mutex};

use b9::advisory::{AdvisoryAction, AdvisoryContext};
use b9::providers::advisory::{AdvisoryClient, AdvisoryProvider};
use b9::transport::{
    ExecutorError, HttpClient, HttpExecutor, HttpHeader, HttpResponse, ValidatedRequest,
};

struct ReplyExecutor {
    request: Mutex<Option<ValidatedRequest>>,
    response: HttpResponse,
}

impl HttpExecutor for ReplyExecutor {
    fn execute(&self, request: ValidatedRequest) -> Result<HttpResponse, ExecutorError> {
        *self.request.lock().unwrap() = Some(request);
        Ok(self.response.clone())
    }
}

fn context() -> AdvisoryContext {
    AdvisoryContext {
        lineup_candidates: vec![AdvisoryAction {
            id: "lineup-0".into(),
            summary: "Start Ada".into(),
        }],
        roster_moves: Vec::new(),
    }
}

#[test]
fn adapters_normalize_each_provider_request_without_exposing_credentials() {
    for provider in [
        AdvisoryProvider::Gemini,
        AdvisoryProvider::Groq,
        AdvisoryProvider::Mistral,
        AdvisoryProvider::Claude,
        AdvisoryProvider::OpenAi,
    ] {
        let content = r#"{\"confirmations\":[\"ok\"],\"urgent\":[],\"overnight\":[],\"risks\":[]}"#;
        let response = match provider {
            AdvisoryProvider::Gemini => {
                format!(r#"{{"candidates":[{{"content":{{"parts":[{{"text":"{content}"}}]}}}}]}}"#)
            }
            AdvisoryProvider::Claude => format!(r#"{{"content":[{{"text":"{content}"}}]}}"#),
            _ => format!(r#"{{"choices":[{{"message":{{"content":"{content}"}}}}]}}"#),
        };
        let executor = Arc::new(ReplyExecutor {
            request: Mutex::new(None),
            response: HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: response.into_bytes(),
            },
        });
        let client = AdvisoryClient::new(Arc::new(HttpClient::new(executor.clone())));
        assert_eq!(
            client
                .complete(provider, "test-model", "private-token", &context())
                .unwrap()
                .confirmations,
            ["ok"]
        );
        let request = executor.request.lock().unwrap();
        let request = request.as_ref().unwrap();
        assert_eq!(request.timeout().as_secs(), 15);
        assert_eq!(request.body_limit(), 64 * 1024);
        assert!(request.body().len() < 32 * 1024);
        assert!(!format!("{request:?}").contains("private-token"));
        assert!(
            request
                .headers()
                .iter()
                .any(|header| header.name == "authorization"
                    || header.name == "x-api-key"
                    || header.name == "x-goog-api-key")
        );
    }
}

#[test]
fn provider_selection_rejects_unknown_names_without_requests() {
    assert!(AdvisoryProvider::parse("unsupported").is_err());
    assert_eq!(
        AdvisoryProvider::parse("Groq/Llama").unwrap(),
        AdvisoryProvider::Groq
    );
}

#[test]
fn provider_errors_do_not_echo_credentials() {
    let executor = Arc::new(ReplyExecutor {
        request: Mutex::new(None),
        response: HttpResponse {
            status: 401,
            headers: vec![HttpHeader {
                name: "x".into(),
                value: "y".into(),
            }],
            body: Vec::new(),
        },
    });
    let client = AdvisoryClient::new(Arc::new(HttpClient::new(executor)));
    let error = client
        .complete(AdvisoryProvider::OpenAi, "", "secret-value", &context())
        .unwrap_err()
        .to_string();
    assert!(error.contains("HTTP 401"));
    assert!(!error.contains("secret-value"));
}
