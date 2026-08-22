use std::process::{Command, Output};

use skout::cli::render_root_help;
use skout::config::{Config, write_at};
use skout::domain::{FantasyPlayer, FantasyRosterSlot, FantasyTeam, League, Position, ScoringType};
use skout::store::{FantasySnapshotWrite, IdentityCandidate, PositionWrite, Store};
use skout::terminal::HelpColorMode;

fn skout(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_skout"))
        .args(arguments)
        .output()
        .expect("run skout")
}

fn available_pitcher_home() -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("temporary HOME");
    let config_dir = home.path().join(".config/skout");
    write_at(
        config_dir.join("config.json"),
        &Config {
            current_league: "mlb.l.1".into(),
            current_team_key: "mlb.l.1.t.1".into(),
            pull_public_league_id: String::new(),
        },
    )
    .unwrap();
    let mut players = (1..=25)
        .map(|id| FantasyPlayer {
            yahoo_player_id: id,
            name: format!("Pitcher {id}"),
            mlb_team: "NYY".into(),
            display_position: "SP".into(),
            position_type: "P".into(),
            eligible_positions: vec![Position::StartingPitcher],
            injury_status: String::new(),
            percent_owned: Some(1.0),
            percentage_started: Some(1.0),
            yahoo_rank: Some(id),
        })
        .collect::<Vec<_>>();
    players.push(FantasyPlayer {
        yahoo_player_id: 12540,
        name: "Bryce Miller".into(),
        mlb_team: "SEA".into(),
        display_position: "SP".into(),
        position_type: "P".into(),
        eligible_positions: vec![Position::StartingPitcher, Position::Other("P".into())],
        injury_status: String::new(),
        percent_owned: Some(54.0),
        percentage_started: Some(50.0),
        yahoo_rank: Some(254),
    });
    let mut store = Store::open_at(config_dir.join("skout.db")).unwrap();
    store
        .replace_fantasy_snapshot(&FantasySnapshotWrite {
            league: League {
                league_key: "mlb.l.1".into(),
                name: "CLI Test League".into(),
                season: 2026,
                num_teams: 1,
                scoring_type: ScoringType::HeadToHead,
                roster_positions: vec![Position::StartingPitcher],
                batting_categories: Vec::new(),
                pitching_categories: Vec::new(),
            },
            current_week: Some(1),
            categories: Vec::new(),
            positions: vec![PositionWrite {
                position: "SP".into(),
                count: 1,
            }],
            teams: vec![FantasyTeam {
                team_key: "mlb.l.1.t.1".into(),
                league_key: "mlb.l.1".into(),
                team_id: 1,
                name: "Operators".into(),
                manager_name: "Operator".into(),
                is_owned_by_current_login: true,
                waiver_priority: 1,
                faab_balance: 100,
                wins: 0,
                losses: 0,
                ties: 0,
                moves: 0,
                rank: 1,
            }],
            players,
            slots: vec![FantasyRosterSlot {
                team_key: "mlb.l.1.t.1".into(),
                yahoo_player_id: 1,
                slot_position: Position::StartingPitcher,
            }],
        })
        .unwrap();
    assert_eq!(
        store
            .reconcile_mlb_identities(&[IdentityCandidate {
                mlbam_id: 682243,
                name: "Bryce Miller".into(),
                team: "SEA".into(),
                role: "P".into(),
            }])
            .unwrap(),
        1
    );
    store
        .save_command_snapshot("player-game-log", "mlb", "682243", "v1", "[]")
        .unwrap();
    drop(store);
    home
}

#[test]
fn root_help_forms_share_the_golden_surface() {
    let default = skout(&[]);
    assert!(default.status.success());
    assert!(default.stderr.is_empty());
    let help = String::from_utf8(default.stdout).expect("UTF-8 root help");
    assert_eq!(help, render_root_help("0.22.1", HelpColorMode::Plain));

    for form in [["-h"].as_slice(), ["--help"].as_slice(), ["-?"].as_slice()] {
        let output = skout(form);
        assert!(output.status.success(), "help form {form:?}");
        assert_eq!(output.stdout, help.as_bytes(), "help form {form:?}");
        assert!(output.stderr.is_empty(), "help form {form:?}");
    }
}

#[test]
fn root_help_lists_every_command_specific_flag() {
    let root = String::from_utf8(skout(&["--help"]).stdout).unwrap();
    for (command, flags) in [
        ("sync", &["-T, --team"][..]),
        ("m", &["-w, --week", "-W, --weekly", "-D, --day"]),
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
        let output = skout(&[command, "--help"]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("Usage: skout i")
        );
    }
}

#[test]
fn version_forms_print_the_exact_utility_contract() {
    for form in ["-v", "--version"] {
        let output = skout(&[form]);
        assert!(output.status.success());
        assert_eq!(output.stdout, b"skout 0.22.1\n");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn glossary_commands_work_without_the_repository_as_working_directory() {
    for command in ["whatis", "i"] {
        let output = Command::new(env!("CARGO_BIN_EXE_skout"))
            .args([command, "pa"])
            .current_dir(std::env::temp_dir())
            .output()
            .expect("run installed-shape skout glossary");
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 glossary entry");
        assert!(stdout.starts_with("Plate Appearance (pa) [baseball]\nAliases: PA\n"));
        assert!(!stdout.contains("\u{1b}["));
    }
}

#[test]
fn full_glossary_is_plain_and_grouped() {
    let output = skout(&["i"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 glossary");
    let baseball = stdout.find("BASEBALL\n").expect("baseball banner");
    let fantasy = stdout.find("FANTASY\n").expect("fantasy banner");
    let skout_group = stdout.find("SKOUT\n").expect("skout banner");
    let stat = stdout.find("STAT\n").expect("stat banner");
    assert!(baseball < fantasy && fantasy < skout_group && skout_group < stat);
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
        (
            &["i", "pa", "extra"][..],
            2,
            "Usage: skout i [OPTIONS] [TERM]",
        ),
        (&["unknown"][..], 2, "unrecognized subcommand"),
    ];
    for (arguments, code, message) in cases {
        let output = skout(arguments);
        assert_eq!(output.status.code(), Some(code), "arguments {arguments:?}");
        assert!(output.stdout.is_empty(), "arguments {arguments:?}");
        let stderr = String::from_utf8(output.stderr).expect("UTF-8 error");
        assert!(stderr.contains(message), "stderr {stderr:?}");
    }
}

#[test]
fn fantasy_commands_have_help_without_side_effects() {
    for command in ["st", "sync", "reset", "m", "i", "whatis"] {
        let output = skout(&[command, "--help"]);
        assert!(output.status.success(), "command {command}");
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8(output.stdout).unwrap().contains("Usage:"));
    }
}

#[test]
fn sync_help_exposes_only_the_team_selection_flag() {
    let sync = String::from_utf8(skout(&["sync", "--help"]).stdout).unwrap();
    assert!(!sync.contains("--force"));
    assert!(sync.contains("-T, --team"));
    assert!(sync.contains("Select the primary fantasy team"));
}

#[test]
fn mlb_commands_have_force_help() {
    for command in ["t", "tt", "sp"] {
        let output = skout(&[command, "--help"]);
        assert!(output.status.success(), "command {command}");
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("-f, --force"));
    }
}

#[test]
fn status_is_local_first() {
    let home = tempfile::tempdir().expect("temporary HOME");
    let output = Command::new(env!("CARGO_BIN_EXE_skout"))
        .args(["st"])
        .env("HOME", home.path())
        .output()
        .expect("run local status");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 status");
    assert!(stdout.contains("Yahoo: public endpoints"));
    assert!(stdout.contains("No local snapshot; run skout sync."));
}

#[test]
fn retired_cli_surfaces_are_absent() {
    let root = String::from_utf8(skout(&["--help"]).stdout).unwrap();
    for retired in [
        "login",
        "logout",
        "pp",
        "pull-public",
        "start",
        "restart",
        "log",
        "_daemon",
        "stop",
        "help",
        "lm",
    ] {
        assert!(
            !root.lines().any(|line| {
                line.strip_prefix("  ")
                    .and_then(|value| value.split_whitespace().next())
                    == Some(retired)
            }),
            "retired surface remains: {retired}"
        );
    }
    assert!(!root.contains("--oauth"));
    assert!(!root.contains("--advise"));
    for command in [
        "login",
        "logout",
        "pp",
        "pull-public",
        "start",
        "restart",
        "log",
        "_daemon",
        "stop",
        "help",
        "lm",
    ] {
        assert_eq!(skout(&[command]).status.code(), Some(2), "{command}");
    }
    assert_eq!(skout(&["m", "--advise"]).status.code(), Some(2));
}

#[test]
fn m_team_argument_fails_noninteractively_with_actionable_guidance_when_no_league_is_selected() {
    let home = tempfile::tempdir().expect("temporary HOME");
    let output = Command::new(env!("CARGO_BIN_EXE_skout"))
        .args(["m", "Yankees"])
        .env("HOME", home.path())
        .output()
        .expect("run m with a team argument and no configured league");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 m diagnostics");
    assert!(stderr.contains("skout st -l"));
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
        let output = Command::new(env!("CARGO_BIN_EXE_skout"))
            .args(arguments)
            .env("HOME", home.path())
            .output()
            .expect("run skout without storage");
        assert!(output.status.success(), "arguments {arguments:?}");
        assert!(!home.path().join(".config/skout/skout.db").exists());
    }
}

#[test]
fn pool_help_preserves_the_existing_waiver_surface() {
    for command in ["h", "p"] {
        let output = Command::new(env!("CARGO_BIN_EXE_skout"))
            .args([command, "--help"])
            .output()
            .expect("run player-pool help");
        assert!(output.status.success());
        let help = String::from_utf8_lossy(&output.stdout);
        assert!(help.contains("--waiver"));
        assert!(help.contains("Show available Yahoo pickup players"));
    }
}

#[test]
fn available_pitcher_survives_executable_lookup_and_expanded_waiver_view() {
    let home = available_pitcher_home();
    let expanded = Command::new(env!("CARGO_BIN_EXE_skout"))
        .args(["p", "1000", "-w"])
        .env("HOME", home.path())
        .output()
        .expect("run expanded pitcher waiver view");
    assert!(expanded.status.success());
    assert!(
        String::from_utf8(expanded.stdout)
            .unwrap()
            .contains("Bryce Miller")
    );

    let default = Command::new(env!("CARGO_BIN_EXE_skout"))
        .args(["p", "-w"])
        .env("HOME", home.path())
        .output()
        .expect("run default pitcher waiver view");
    assert!(default.status.success());
    assert!(
        !String::from_utf8(default.stdout)
            .unwrap()
            .contains("Bryce Miller")
    );

    let exact = Command::new(env!("CARGO_BIN_EXE_skout"))
        .args(["p", "bryce miller"])
        .env("HOME", home.path())
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("ALL_PROXY", "http://127.0.0.1:9")
        .output()
        .expect("run exact pitcher lookup");
    assert!(exact.status.success());
    assert!(
        String::from_utf8(exact.stdout)
            .unwrap()
            .contains("Bryce Miller")
    );
}
