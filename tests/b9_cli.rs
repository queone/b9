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
        "Usage: b9 [OPTIONS] [COMMAND]\n\nFantasy Baseball Advisor\n\nCommands:\n  whatis  Look up a term in the b9 glossary [aliases: i]\n  help    Print help\n\nOptions:\n  -h, --help     Print help\n  -v, --version  Print version\n"
    );

    for form in [["-h"].as_slice(), ["--help"].as_slice(), ["-?"].as_slice()] {
        let output = b9(form);
        assert!(output.status.success(), "help form {form:?}");
        assert_eq!(output.stdout, help.as_bytes(), "help form {form:?}");
        assert!(output.stderr.is_empty(), "help form {form:?}");
    }
    for absent in ["league", "debug", "login", "sync"] {
        assert!(!help.contains(absent));
    }
}

#[test]
fn version_forms_print_the_exact_utility_contract() {
    for form in ["-v", "--version"] {
        let output = b9(&[form]);
        assert!(output.status.success());
        assert_eq!(output.stdout, b"b9 0.1.0\n");
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
    let output = b9(&["whatis"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 glossary");
    let baseball = stdout.find("BASEBALL\n").expect("baseball banner");
    let fantasy = stdout.find("FANTASY\n").expect("fantasy banner");
    let skout = stdout.find("SKOUT\n").expect("skout banner");
    let stat = stdout.find("STAT\n").expect("stat banner");
    assert!(baseball < fantasy && fantasy < skout && skout < stat);
    assert!(!stdout.contains("\u{1b}["));
}

#[test]
fn lookup_and_parser_errors_use_stderr_and_classified_exits() {
    let cases = [
        (&["whatis", "   "][..], 1, "empty term"),
        (&["whatis", "definitely-not-a-key"][..], 1, "closest keys:"),
        (&["whatis", "run"][..], 1, "is ambiguous"),
        (&["whatis", "pa", "extra"][..], 2, "unexpected value"),
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
