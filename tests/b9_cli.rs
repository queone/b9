use std::process::{Command, Output};

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
    assert_eq!(
        help,
        "b9 v0.13.0\nFantasy Baseball Advisor\n\nUSAGE\n  b9 <command> [flags]\n\nCOMMANDS\n  login                        Authenticate with Yahoo\n  logout                       Remove Yahoo authentication\n  st                           Show status and select a league\n  sync                         Synchronize the selected league\n  m                            Show the baseline weekly matchup\n  t [team]                     Show MLB 40-man rosters\n  tt                           Show MLB standings and team totals\n  sp                           Show the three-day probable-pitcher slate\n  i [term]                     Look up a term in the b9 glossary\n  help                         Print this help\n\nFLAGS\n  -l, --league <key>           Yahoo league key\n  -d, --debug                  Print operation diagnostics\n  -v, --version                Print version\n  -h, -?, --help               Print this help\n"
    );

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
fn command_specific_help_remains_the_shipped_clap_error() {
    for command in ["whatis", "i"] {
        let output = b9(&[command, "--help"]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            "error: unexpected argument '--help' found\n\n  tip: to pass '--help' as a value, use '-- --help'\n\nUsage: b9 i [OPTIONS] [TERM]\n"
        );
    }
}

#[test]
fn version_forms_print_the_exact_utility_contract() {
    for form in ["-v", "--version"] {
        let output = b9(&[form]);
        assert!(output.status.success());
        assert_eq!(output.stdout, b"b9 0.13.0\n");
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
    for command in ["login", "logout", "st", "sync", "m"] {
        let output = b9(&[command, "--help"]);
        assert!(output.status.success(), "command {command}");
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8(output.stdout).unwrap().contains("Usage:"));
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
