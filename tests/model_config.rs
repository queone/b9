use b9::config::Config;
use b9::model_config::{
    PROVIDERS, apply_selection, commit_validated_selection, parse_provider_choice,
    select_openai_model,
};

#[test]
fn provider_defaults_and_disabled_state_are_deterministic() {
    let mut config = Config::default();
    apply_selection(&mut config, "gemini", "").unwrap();
    assert_eq!(config.advisory_provider, "gemini");
    assert!(!config.advisory_model.is_empty());
    apply_selection(&mut config, "none", "ignored").unwrap();
    assert!(config.advisory_provider.is_empty());
    assert!(config.advisory_model.is_empty());
}

#[test]
fn invalid_provider_does_not_mutate_config() {
    let mut config = Config {
        advisory_provider: "openai".into(),
        advisory_model: "gpt-test".into(),
        ..Config::default()
    };
    assert!(apply_selection(&mut config, "unknown", "model").is_err());
    assert_eq!(config.advisory_provider, "openai");
    assert_eq!(config.advisory_model, "gpt-test");
}

#[test]
fn validation_failure_preserves_selection_and_never_stores_key() {
    let mut config = Config {
        advisory_provider: "openai".into(),
        advisory_model: "gpt-old".into(),
        ..Config::default()
    };
    let mut stored = false;
    assert!(
        commit_validated_selection(
            &mut config,
            "openai",
            "gpt-new",
            "private-token",
            true,
            |_, _, _| Err("rejected".into()),
            |_, _| {
                stored = true;
                Ok(())
            },
        )
        .is_err()
    );
    assert!(!stored);
    assert_eq!(config.advisory_model, "gpt-old");
}

#[test]
fn validated_new_key_is_stored_without_entering_config() {
    let mut config = Config::default();
    let mut stored = String::new();
    commit_validated_selection(
        &mut config,
        "claude",
        "model",
        "private-token",
        true,
        |_, _, _| Ok(()),
        |_, credential| {
            stored = credential.into();
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(stored, "private-token");
    assert_eq!(config.advisory_provider, "claude");
    assert!(
        !serde_json::to_string(&config)
            .unwrap()
            .contains("private-token")
    );
}

#[test]
fn provider_menu_cancellation_and_every_selection_are_deterministic() {
    assert_eq!(parse_provider_choice("  ").unwrap(), None);
    for (index, provider) in PROVIDERS.iter().enumerate() {
        assert_eq!(
            parse_provider_choice(&(index + 1).to_string()).unwrap(),
            Some(*provider)
        );
    }
    assert!(parse_provider_choice("99").is_err());
}

#[test]
fn openai_discovery_failure_or_invalid_choice_preserves_current_model() {
    let models = vec!["gpt-new".to_owned()];
    assert_eq!(
        select_openai_model("gpt-current", "gpt-default", &[], None),
        "gpt-current"
    );
    assert_eq!(
        select_openai_model("gpt-current", "gpt-default", &models, Some(99)),
        "gpt-current"
    );
    assert_eq!(
        select_openai_model("gpt-current", "gpt-default", &models, Some(1)),
        "gpt-new"
    );
}
