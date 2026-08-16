#[test]
fn mlb_command_modules_keep_provider_payloads_out_of_cli_and_rendering() {
    let cli = include_str!("../src/cli.rs");
    let display = include_str!("../src/mlb_display.rs");
    for forbidden in ["reqwest", "rusqlite", "serde_json::Value", "HttpRequest"] {
        assert!(!cli.contains(forbidden));
        assert!(!display.contains(forbidden));
    }
}
