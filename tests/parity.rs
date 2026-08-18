use std::fs;
use std::path::Path;
use std::process::Command;

const PUBLIC_COMMANDS: &[&str] = &[
    "st", "sync", "reset", "m", "t", "tt", "sp", "r", "rt", "h", "p", "i",
];

fn repository_file(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
}

#[test]
fn retained_command_inventory_is_visible_and_help_is_reachable() {
    let root = Command::new(env!("CARGO_BIN_EXE_b9"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(root.status.success());
    let root = String::from_utf8(root.stdout).unwrap();
    for command in PUBLIC_COMMANDS {
        assert!(
            root.lines()
                .any(|line| line.trim_start().starts_with(command)),
            "missing public command {command}"
        );
        let help = Command::new(env!("CARGO_BIN_EXE_b9"))
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(help.status.success(), "help failed for {command}");
        assert!(help.stderr.is_empty(), "help wrote stderr for {command}");
    }
    assert!(!root.contains("_daemon"));
    let alias = Command::new(env!("CARGO_BIN_EXE_b9"))
        .args(["whatis", "--help"])
        .output()
        .unwrap();
    assert!(alias.status.success());
    assert!(
        String::from_utf8(alias.stdout)
            .unwrap()
            .contains("Usage: b9 i")
    );
}

#[test]
fn runtime_product_naming_is_b9_owned() {
    for path in [
        "src/cli.rs",
        "src/config.rs",
        "src/operations.rs",
        "src/sync.rs",
    ] {
        for line in repository_file(path).lines() {
            if line.contains("skout") {
                assert!(
                    line.contains("legacy")
                        || line.contains("predecessor")
                        || line.contains("Path::new(\"skout\")")
                        || line.contains(".join(\"skout\")"),
                    "unclassified skout runtime reference in {path}: {line}"
                );
            }
        }
    }
}

#[test]
fn closure_documents_reject_stale_delivery_claims() {
    for path in [
        "README.md",
        "arch.md",
        "plan.md",
        "docs/api-espn.md",
        "docs/api-mlbam.md",
        "docs/api-oddsshark.md",
        "docs/api-yahoo.md",
        "docs/glossary.md",
    ] {
        let document = repository_file(path);
        assert!(
            !document.contains("remaining public commands"),
            "stale command claim in {path}"
        );
        assert!(
            !document.contains("b9 already supplants skout"),
            "unconditional replacement claim in {path}"
        );
    }
}
