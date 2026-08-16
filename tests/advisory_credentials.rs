use b9::advisory_credentials::{
    AdvisoryCredentialError, AdvisoryCredentialStore, load_credential_from, selected_provider,
};
use b9::config::Config;

#[test]
fn empty_provider_is_not_selected() {
    assert_eq!(selected_provider(&Config::default()), None);
    assert_eq!(
        selected_provider(&Config {
            advisory_provider: "openai".into(),
            ..Config::default()
        }),
        Some("openai")
    );
}

struct Store(Option<String>);

impl AdvisoryCredentialStore for Store {
    fn load(&self, account: &str) -> Result<Option<String>, AdvisoryCredentialError> {
        assert_eq!(account, "advisory-openai-api-key");
        Ok(self.0.clone())
    }
}

#[test]
fn credential_recovery_is_provider_scoped_and_handles_missing_entries() {
    assert_eq!(load_credential_from("OpenAI", &Store(None)).unwrap(), None);
    assert_eq!(
        load_credential_from("openai", &Store(Some("opaque".into()))).unwrap(),
        Some("opaque".into())
    );
}
