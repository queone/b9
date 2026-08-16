//! Interactive, secret-safe advisory provider configuration.

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::sync::Arc;

use crate::config::{self, Config};
use crate::providers::advisory::{AdvisoryClient, AdvisoryProvider};
use crate::transport::HttpClient;

/// One selectable advisory provider, including the disabled state.
pub const PROVIDERS: &[&str] = &["none", "gemini", "groq", "mistral", "claude", "openai"];

/// One secret-safe model-configuration failure.
#[derive(Debug)]
pub struct ModelConfigError(String);

impl ModelConfigError {
    fn new(operation: &str, detail: impl fmt::Display) -> Self {
        Self(format!("lm: {operation}: {detail}; retry configuration"))
    }
}

impl fmt::Display for ModelConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ModelConfigError {}

/// Parse one deterministic provider-menu response; blank input cancels.
pub fn parse_provider_choice(input: &str) -> Result<Option<&'static str>, ModelConfigError> {
    if input.trim().is_empty() {
        return Ok(None);
    }
    input
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|value| (1..=PROVIDERS.len()).contains(value))
        .map(|value| Some(PROVIDERS[value - 1]))
        .ok_or_else(|| ModelConfigError::new("select provider", "enter a displayed number"))
}

/// Select a discovered OpenAI model while retaining a prior selection on any unusable result.
#[must_use]
pub fn select_openai_model(
    current: &str,
    default: &str,
    discovered: &[String],
    selection: Option<usize>,
) -> String {
    selection
        .and_then(|value| discovered.get(value.saturating_sub(1)))
        .cloned()
        .or_else(|| (!current.trim().is_empty()).then(|| current.trim().to_owned()))
        .unwrap_or_else(|| default.to_owned())
}

/// Apply one validated provider/model selection without storing credentials.
pub fn apply_selection(
    config: &mut Config,
    provider: &str,
    model: &str,
) -> Result<(), ModelConfigError> {
    let provider = provider.trim().to_ascii_lowercase();
    if !PROVIDERS.contains(&provider.as_str()) {
        return Err(ModelConfigError::new(
            "select provider",
            "unsupported provider",
        ));
    }
    if provider == "none" {
        config.advisory_provider.clear();
        config.advisory_model.clear();
    } else {
        let parsed = AdvisoryProvider::parse(&provider)
            .map_err(|error| ModelConfigError::new("select provider", error))?;
        config.advisory_provider = provider;
        config.advisory_model = if model.trim().is_empty() {
            parsed.default_model().into()
        } else {
            model.trim().into()
        };
    }
    Ok(())
}

/// Validate and commit one provider selection, storing only a newly entered credential.
pub fn commit_validated_selection(
    config: &mut Config,
    provider: &str,
    model: &str,
    credential: &str,
    entered_credential: bool,
    validate: impl FnOnce(&str, &str, &str) -> Result<(), String>,
    store: impl FnOnce(&str, &str) -> Result<(), String>,
) -> Result<(), ModelConfigError> {
    validate(provider, model, credential)
        .map_err(|error| ModelConfigError::new("validate credential", error))?;
    if entered_credential {
        store(provider, credential)
            .map_err(|error| ModelConfigError::new("store credential", error))?;
    }
    apply_selection(config, provider, model)
}

/// Configure the advisory provider interactively on a real terminal.
pub fn configure() -> Result<String, ModelConfigError> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(ModelConfigError::new(
            "open selector",
            "an interactive terminal is required",
        ));
    }
    let mut config = config::read().map_err(|error| ModelConfigError::new("read config", error))?;
    println!("Select advisory provider:");
    for (index, provider) in PROVIDERS.iter().enumerate() {
        println!("  {}. {}", index + 1, provider);
    }
    print!("Choice (blank cancels): ");
    io::stdout()
        .flush()
        .map_err(|error| ModelConfigError::new("write selector", error))?;
    let mut choice = String::new();
    io::stdin()
        .read_line(&mut choice)
        .map_err(|error| ModelConfigError::new("read selector", error))?;
    let Some(provider_name) = parse_provider_choice(&choice)? else {
        return Ok("LLM configuration cancelled.\n".into());
    };
    if provider_name == "none" {
        apply_selection(&mut config, provider_name, "")?;
        config::write(&config).map_err(|error| ModelConfigError::new("save config", error))?;
        return Ok("LLM advisory disabled.\n".into());
    }

    let provider = AdvisoryProvider::parse(provider_name)
        .map_err(|error| ModelConfigError::new("select provider", error))?;
    let existing = crate::advisory_credentials::load_credential(provider_name)
        .map_err(|error| ModelConfigError::new("read credential", error))?;
    let (credential, entered_credential) = if let Some(value) = existing {
        (value, false)
    } else {
        let value = crate::terminal::read_secret("API key: ")
            .map_err(|error| ModelConfigError::new("read credential", error))?;
        if value.trim().is_empty() {
            return Err(ModelConfigError::new("read credential", "key is empty"));
        }
        (value, true)
    };
    let http = Arc::new(
        HttpClient::production()
            .map_err(|error| ModelConfigError::new("initialize transport", error))?,
    );
    let client = AdvisoryClient::new(http);
    let mut model = provider.default_model().to_owned();
    if provider == AdvisoryProvider::OpenAi {
        match client.discover_openai_models(&credential) {
            Ok(models) if !models.is_empty() => {
                println!("Select OpenAI model:");
                for (index, value) in models.iter().enumerate() {
                    println!("  {}. {}", index + 1, value);
                }
                print!("Choice (blank keeps {}): ", config.advisory_model);
                io::stdout()
                    .flush()
                    .map_err(|error| ModelConfigError::new("write model selector", error))?;
                let mut choice = String::new();
                io::stdin()
                    .read_line(&mut choice)
                    .map_err(|error| ModelConfigError::new("read model selector", error))?;
                model = select_openai_model(
                    &config.advisory_model,
                    provider.default_model(),
                    &models,
                    choice.trim().parse().ok(),
                );
            }
            Ok(_) | Err(_) => {
                model = select_openai_model(
                    &config.advisory_model,
                    provider.default_model(),
                    &[],
                    None,
                );
            }
        }
    }
    commit_validated_selection(
        &mut config,
        provider_name,
        &model,
        &credential,
        entered_credential,
        |_, model, credential| {
            client
                .validate_credential(provider, model, credential)
                .map_err(|error| error.to_string())
        },
        |provider, credential| {
            crate::advisory_credentials::store_credential(provider, credential)
                .map_err(|error| error.to_string())
        },
    )?;
    config::write(&config).map_err(|error| ModelConfigError::new("save config", error))?;
    Ok(format!("Provider set to {provider_name}, model {model}.\n"))
}
