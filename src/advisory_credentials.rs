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
}

/// Read the selected provider from private configuration without reading credentials.
pub fn selected_provider(config: &Config) -> Option<&str> {
    (!config.advisory_provider.trim().is_empty()).then_some(config.advisory_provider.trim())
}

/// Read an advisory credential from the operating-system keyring.
pub fn load_credential(provider: &str) -> Result<Option<String>, AdvisoryCredentialError> {
    load_credential_from(provider, &KeyringCredentialStore)
}

/// Load one provider credential through an injected secure-store boundary.
pub fn load_credential_from(
    provider: &str,
    store: &impl AdvisoryCredentialStore,
) -> Result<Option<String>, AdvisoryCredentialError> {
    let account = format!("advisory-{}-api-key", provider.trim().to_ascii_lowercase());
    store.load(&account)
}
