//! Transitional deletion-only cleanup for the retired Yahoo OAuth credential.

use std::fmt;

const KEYRING_SERVICE: &str = "b9";
const KEYRING_ACCOUNT: &str = "yahoo-oauth-token";

/// One legacy Yahoo credential cleanup failure.
#[derive(Debug)]
pub struct LegacyYahooCredentialError(&'static str);

impl fmt::Display for LegacyYahooCredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "remove legacy Yahoo credential: {}; remove it from the operating-system keychain manually and retry",
            self.0
        )
    }
}

impl std::error::Error for LegacyYahooCredentialError {}

#[derive(Clone, Copy)]
enum DeleteOutcome {
    Deleted,
    Missing,
}

fn delete_with(
    delete: impl FnOnce(&str, &str) -> Result<DeleteOutcome, &'static str>,
) -> Result<(), LegacyYahooCredentialError> {
    match delete(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        Ok(DeleteOutcome::Deleted | DeleteOutcome::Missing) => Ok(()),
        Err(message) => Err(LegacyYahooCredentialError(message)),
    }
}

/// Delete the exact retired Yahoo credential without reading or recreating it.
pub fn delete_legacy_yahoo_credential() -> Result<(), LegacyYahooCredentialError> {
    if keyring::Entry::store_status().is_err() {
        return Err(LegacyYahooCredentialError(
            "secure credential storage is unavailable",
        ));
    }
    delete_with(|service, account| {
        let entry = keyring::Entry::new(service, account)
            .map_err(|_| "the keychain entry could not be opened")?;
        match entry.delete_credential() {
            Ok(()) => Ok(DeleteOutcome::Deleted),
            Err(keyring::Error::NoEntry) => Ok(DeleteOutcome::Missing),
            Err(_) => Err("the keychain denied credential deletion"),
        }
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::{DeleteOutcome, delete_with};

    #[test]
    fn cleanup_exposes_only_exact_deletion_and_accepts_absence() {
        let call = RefCell::new(None);
        delete_with(|service, account| {
            *call.borrow_mut() = Some((service.to_owned(), account.to_owned()));
            Ok(DeleteOutcome::Missing)
        })
        .unwrap();
        assert_eq!(
            call.into_inner(),
            Some(("b9".into(), "yahoo-oauth-token".into()))
        );
    }

    #[test]
    fn cleanup_reports_manual_recovery_without_credential_contents() {
        let error = delete_with(|_, _| Err("the keychain denied credential deletion"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("keychain manually and retry"));
        assert!(!error.contains("token="));
    }

    #[test]
    fn successful_deletion_is_accepted() {
        delete_with(|_, _| Ok(DeleteOutcome::Deleted)).unwrap();
    }
}
