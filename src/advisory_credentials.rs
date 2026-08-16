//! Noninteractive advisory-provider selection without durable secrets.

use std::fmt;

use crate::config::Config;

const KEYRING_SERVICE: &str = "b9";

/// One noninteractive advisory credential failure that never exposes the secret.
#[derive(Debug)]
pub struct AdvisoryCredentialError(&'static str);

impl fmt::Display for AdvisoryCredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "advisory credential: {}; configure the selected provider credential and retry",
            self.0
        )
    }
}

impl std::error::Error for AdvisoryCredentialError {}

/// Reads opaque advisory credentials without exposing a platform implementation.
pub trait AdvisoryCredentialStore {
    /// Load one provider-scoped credential, returning `None` when it has not been configured.
    fn load(&self, account: &str) -> Result<Option<String>, AdvisoryCredentialError>;

    /// Store one provider-scoped credential without returning it.
    fn store(&self, account: &str, credential: &str) -> Result<(), AdvisoryCredentialError>;
}

struct KeyringCredentialStore;

impl AdvisoryCredentialStore for KeyringCredentialStore {
    fn load(&self, account: &str) -> Result<Option<String>, AdvisoryCredentialError> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, account)
            .map_err(|_| AdvisoryCredentialError("open secure credential entry"))?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(AdvisoryCredentialError("read secure credential entry")),
        }
    }

    fn store(&self, account: &str, credential: &str) -> Result<(), AdvisoryCredentialError> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, account)
            .map_err(|_| AdvisoryCredentialError("open secure credential entry"))?;
        entry
            .set_password(credential)
            .map_err(|_| AdvisoryCredentialError("write secure credential entry"))
    }
}

/// Read the selected provider from private configuration without reading credentials.
pub fn selected_provider(config: &Config) -> Option<&str> {
    (!config.advisory_provider.trim().is_empty()).then_some(config.advisory_provider.trim())
}

/// Read an advisory credential from the operating-system keyring.
pub fn load_credential(provider: &str) -> Result<Option<String>, AdvisoryCredentialError> {
    let environment = format!(
        "B9_{}_API_KEY",
        provider
            .trim()
            .to_ascii_uppercase()
            .replace(['/', '-'], "_")
    );
    load_credential_with_environment(
        provider,
        std::env::var(environment).ok().as_deref(),
        &KeyringCredentialStore,
    )
}

/// Resolve an injected environment credential before consulting secure storage.
pub fn load_credential_with_environment(
    provider: &str,
    environment: Option<&str>,
    store: &impl AdvisoryCredentialStore,
) -> Result<Option<String>, AdvisoryCredentialError> {
    if let Some(value) = environment.filter(|value| !value.trim().is_empty()) {
        return Ok(Some(value.trim().to_owned()));
    }
    load_credential_from(provider, store)
}

/// Store one provider credential in the operating-system keyring.
pub fn store_credential(provider: &str, credential: &str) -> Result<(), AdvisoryCredentialError> {
    store_credential_with(provider, credential, &KeyringCredentialStore)
}

/// Store one provider credential through an injected secure-store boundary.
pub fn store_credential_with(
    provider: &str,
    credential: &str,
    store: &impl AdvisoryCredentialStore,
) -> Result<(), AdvisoryCredentialError> {
    if credential.trim().is_empty() {
        return Err(AdvisoryCredentialError("refuse empty credential"));
    }
    store.store(&account(provider), credential.trim())
}

/// Load one provider credential through an injected secure-store boundary.
pub fn load_credential_from(
    provider: &str,
    store: &impl AdvisoryCredentialStore,
) -> Result<Option<String>, AdvisoryCredentialError> {
    store.load(&account(provider))
}

fn account(provider: &str) -> String {
    format!("advisory-{}-api-key", provider.trim().to_ascii_lowercase())
}
