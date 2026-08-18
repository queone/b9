use std::process::{Command, Output};

use b9::cli::render_root_help;
use b9::terminal::HelpColorMode;

fn b9(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_b9"))
        .args(arguments)
        .output()
        .expect("run b9")
}

#[test]
fn root_help_forms_share_the_golden_surface() {
    let default = b9(&[]);
    assert!(default.status.success());
    assert!(default.stderr.is_empty());
    let help = String::from_utf8(default.stdout).expect("UTF-8 root help");
    assert_eq!(help, render_root_help("0.22.1", HelpColorMode::Plain));

    for form in [
        ["-h"].as_slice(),
        ["--help"].as_slice(),
        ["-?"].as_slice(),
        ["help"].as_slice(),
    ] {
        let output = b9(form);
        assert!(output.status.success(), "help form {form:?}");
        assert_eq!(output.stdout, help.as_bytes(), "help form {form:?}");
        assert!(output.stderr.is_empty(), "help form {form:?}");
    }
}

#[test]
fn root_help_lists_every_command_specific_flag() {
    let root = String::from_utf8(b9(&["--help"]).stdout).unwrap();
    for (command, flags) in [
        ("sync", &["-f, --force", "-T, --team"][..]),
        ("log", &["-n, --lines", "-f, --follow", "-p, --path"]),
        (
            "m",
            &["-w, --week", "-W, --weekly", "-D, --day", "-a, --advise"],
        ),
        ("t", &["-f, --force"]),
        ("tt", &["-f, --force"]),
        ("sp", &["-f, --force"]),
        ("rt", &["-w, --weekly"]),
        ("h", &["-s, --sort", "-p, --position", "-w, --waiver"]),
        ("p", &["-s, --sort", "-p, --position", "-w, --waiver"]),
    ] {
        let lines = root.lines().collect::<Vec<_>>();
        let command_index = lines
            .iter()
            .position(|line| {
                !line.starts_with("    ")
                    && line
                        .strip_prefix("  ")
                        .and_then(|value| value.split_whitespace().next())
                        == Some(command)
            })
            .unwrap_or_else(|| panic!("missing root command row for {command}"));
        let block = lines[command_index + 1..]
            .iter()
            .take_while(|line| line.starts_with("    "))
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        for flag in flags {
            assert!(block.contains(flag), "missing {command} flag {flag}");
        }
    }
}

#[test]
fn glossary_aliases_share_command_help() {
    for command in ["whatis", "i"] {
        let output = b9(&[command, "--help"]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("Usage: b9 i")
        );
    }
}

#[test]
fn version_forms_print_the_exact_utility_contract() {
    for form in ["-v", "--version"] {
        let output = b9(&[form]);
        assert!(output.status.success());
        assert_eq!(output.stdout, b"b9 0.22.1\n");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn glossary_commands_work_without_the_repository_as_working_directory() {
    for command in ["whatis", "i"] {
        let output = Command::new(env!("CARGO_BIN_EXE_b9"))
            .args([command, "pa"])
            .current_dir(std::env::temp_dir())
            .output()
            .expect("run installed-shape b9 glossary");
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 glossary entry");
        assert!(stdout.starts_with("Plate Appearance (pa) [baseball]\nAliases: PA\n"));
        assert!(!stdout.contains("\u{1b}["));
    }
}

#[test]
fn full_glossary_is_plain_and_grouped() {
    let output = b9(&["i"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 glossary");
    let baseball = stdout.find("BASEBALL\n").expect("baseball banner");
    let fantasy = stdout.find("FANTASY\n").expect("fantasy banner");
    let b9_group = stdout.find("B9\n").expect("b9 banner");
    let stat = stdout.find("STAT\n").expect("stat banner");
    assert!(baseball < fantasy && fantasy < b9_group && b9_group < stat);
    assert!(!stdout.contains(&["SK", "OUT"].concat()));
    assert!(!stdout.contains("\u{1b}["));
}

#[test]
fn lookup_and_parser_errors_use_stderr_and_classified_exits() {
    let cases = [
        (&["i", "   "][..], 1, "i: empty term"),
        (
            &["i", "definitely-not-a-key"][..],
            1,
            "i: no glossary entry",
        ),
        (&["i", "run"][..], 1, "i: term"),
        (&["i", "pa", "extra"][..], 2, "Usage: b9 i [OPTIONS] [TERM]"),
        (&["unknown"][..], 2, "unrecognized subcommand"),
    ];
    for (arguments, code, message) in cases {
        let output = b9(arguments);
        assert_eq!(output.status.code(), Some(code), "arguments {arguments:?}");
        assert!(output.stdout.is_empty(), "arguments {arguments:?}");
        let stderr = String::from_utf8(output.stderr).expect("UTF-8 error");
        assert!(stderr.contains(message), "stderr {stderr:?}");
    }
}

#[test]
fn fantasy_commands_have_help_without_side_effects() {
    for command in [
        "logout", "st", "sync", "start", "stop", "restart", "log", "reset", "lm", "m", "i",
        "whatis",
    ] {
        let output = b9(&[command, "--help"]);
        assert!(output.status.success(), "command {command}");
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8(output.stdout).unwrap().contains("Usage:"));
    }
}

#[test]
fn operational_arguments_and_noninteractive_model_selection_fail_cleanly() {
    let negative_lines = b9(&["log", "--lines", "-1"]);
    assert_eq!(negative_lines.status.code(), Some(2));
    let model = b9(&["lm"]);
    assert_eq!(model.status.code(), Some(1));
    let stderr = String::from_utf8(model.stderr).unwrap();
    assert!(stderr.contains("interactive terminal is required"));
    assert!(!stderr.contains("API key"));
}

#[test]
fn operational_help_exposes_settled_short_and_long_flags() {
    let sync = String::from_utf8(b9(&["sync", "--help"]).stdout).unwrap();
    assert!(sync.contains("-f, --force"));
    assert!(sync.contains("-T, --team"));
    assert!(sync.contains("Select the primary fantasy team"));
    let log = String::from_utf8(b9(&["log", "--help"]).stdout).unwrap();
    for flag in ["-n, --lines", "-f, --follow", "-p, --path"] {
        assert!(log.contains(flag), "missing {flag}");
    }
}

#[test]
fn mlb_commands_have_force_help_without_yahoo_attribution() {
    for command in ["t", "tt", "sp"] {
        let output = b9(&[command, "--help"]);
        assert!(output.status.success(), "command {command}");
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("-f, --force"));
        assert!(!stdout.contains("Data provided by Yahoo Fantasy Sports."));
    }
}

#[test]
fn status_is_local_first_and_has_no_yahoo_attribution() {
    let home = tempfile::tempdir().expect("temporary HOME");
    let output = Command::new(env!("CARGO_BIN_EXE_b9"))
        .args(["st"])
        .env("HOME", home.path())
        .output()
        .expect("run local status");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 status");
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 status diagnostics");
    assert!(stdout.contains("Yahoo: public endpoints"));
    assert!(stdout.contains("No local snapshot; run b9 sync."));
    assert!(!stderr.contains("Data provided by Yahoo Fantasy Sports."));
}

#[test]
fn retired_oauth_surfaces_are_absent() {
    let root = String::from_utf8(b9(&["--help"]).stdout).unwrap();
    for retired in ["login", "pp", "pull-public", "fetch", "--oauth"] {
        assert!(
            !root.contains(retired),
            "retired surface remains: {retired}"
        );
    }
    for command in ["login", "pp", "pull-public", "fetch"] {
        assert_eq!(b9(&[command]).status.code(), Some(2), "{command}");
    }
}

#[test]
fn m_team_argument_fails_noninteractively_with_actionable_guidance_when_no_league_is_selected() {
    let home = tempfile::tempdir().expect("temporary HOME");
    let output = Command::new(env!("CARGO_BIN_EXE_b9"))
        .args(["m", "Yankees"])
        .env("HOME", home.path())
        .output()
        .expect("run m with a team argument and no configured league");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 m diagnostics");
    assert!(stderr.contains("b9 st -l"));
}

#[test]
fn existing_commands_do_not_create_the_production_database() {
    let home = tempfile::tempdir().expect("temporary HOME");
    for arguments in [
        &[][..],
        &["--help"][..],
        &["i", "pa"][..],
        &["whatis", "pa"][..],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_b9"))
            .args(arguments)
            .env("HOME", home.path())
            .output()
            .expect("run b9 without storage");
        assert!(output.status.success(), "arguments {arguments:?}");
        assert!(!home.path().join(".config/b9/b9.db").exists());
    }
}

#[test]
fn pool_help_preserves_the_existing_waiver_surface() {
    let output = Command::new(env!("CARGO_BIN_EXE_b9"))
        .args(["h", "--help"])
        .output()
        .expect("run hitter help");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("--waiver"));
}
