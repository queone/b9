//! Root command metadata, parsing, dispatch, help, streams, and exit behavior.

use std::ffi::OsString;
use std::process::ExitCode;

use clap::{Arg, ArgAction, Command, error::ErrorKind};

use crate::glossary::{LookupResult, embedded_entries, lookup, render_entry, render_full};
use crate::terminal::{HelpColorMode, detected_help_color_mode, section, subtitle, title};

#[derive(Clone, Copy)]
struct ArgumentDescriptor {
    id: &'static str,
    value_name: &'static str,
}

#[derive(Clone, Copy)]
struct CommandDescriptor {
    name: &'static str,
    display_label: &'static str,
    description: &'static str,
    argument: Option<ArgumentDescriptor>,
    aliases: &'static [&'static str],
    routes_to_root_help: bool,
}

#[derive(Clone, Copy)]
enum FlagAction {
    Help,
    Version,
    SetTrue,
    Value,
}

#[derive(Clone, Copy)]
struct FlagDescriptor {
    id: &'static str,
    short: char,
    long: &'static str,
    display_label: &'static str,
    description: &'static str,
    action: FlagAction,
    routing_aliases: &'static [&'static str],
}

const COMMANDS: &[CommandDescriptor] = &[
    CommandDescriptor {
        name: "login",
        display_label: "login",
        description: "Authenticate with Yahoo",
        argument: None,
        aliases: &[],
        routes_to_root_help: false,
    },
    CommandDescriptor {
        name: "logout",
        display_label: "logout",
        description: "Remove Yahoo authentication",
        argument: None,
        aliases: &[],
        routes_to_root_help: false,
    },
    CommandDescriptor {
        name: "st",
        display_label: "st",
        description: "Show status and select a league",
        argument: None,
        aliases: &[],
        routes_to_root_help: false,
    },
    CommandDescriptor {
        name: "sync",
        display_label: "sync",
        description: "Synchronize the selected league",
        argument: None,
        aliases: &[],
        routes_to_root_help: false,
    },
    CommandDescriptor {
        name: "m",
        display_label: "m",
        description: "Show the baseline weekly matchup",
        argument: None,
        aliases: &[],
        routes_to_root_help: false,
    },
    CommandDescriptor {
        name: "t",
        display_label: "t [team]",
        description: "Show MLB 40-man rosters",
        argument: Some(ArgumentDescriptor {
            id: "team",
            value_name: "TEAM",
        }),
        aliases: &[],
        routes_to_root_help: false,
    },
    CommandDescriptor {
        name: "tt",
        display_label: "tt",
        description: "Show MLB standings and team totals",
        argument: None,
        aliases: &[],
        routes_to_root_help: false,
    },
    CommandDescriptor {
        name: "sp",
        display_label: "sp",
        description: "Show the three-day probable-pitcher slate",
        argument: None,
        aliases: &[],
        routes_to_root_help: false,
    },
    CommandDescriptor {
        name: "r",
        display_label: "r [name]",
        description: "Show a fantasy roster",
        argument: Some(ArgumentDescriptor {
            id: "team",
            value_name: "NAME",
        }),
        aliases: &[],
        routes_to_root_help: false,
    },
    CommandDescriptor {
        name: "rt",
        display_label: "rt",
        description: "Show fantasy roster totals",
        argument: None,
        aliases: &[],
        routes_to_root_help: false,
    },
    CommandDescriptor {
        name: "h",
        display_label: "h [N|name]",
        description: "Browse hitters or show a player",
        argument: Some(ArgumentDescriptor {
            id: "player",
            value_name: "N|NAME",
        }),
        aliases: &[],
        routes_to_root_help: false,
    },
    CommandDescriptor {
        name: "p",
        display_label: "p [N|name]",
        description: "Browse pitchers or show a player",
        argument: Some(ArgumentDescriptor {
            id: "player",
            value_name: "N|NAME",
        }),
        aliases: &[],
        routes_to_root_help: false,
    },
    CommandDescriptor {
        name: "i",
        display_label: "i [term]",
        description: "Look up a term in the b9 glossary",
        argument: Some(ArgumentDescriptor {
            id: "term",
            value_name: "TERM",
        }),
        aliases: &["whatis"],
        routes_to_root_help: false,
    },
    CommandDescriptor {
        name: "help",
        display_label: "help",
        description: "Print this help",
        argument: None,
        aliases: &[],
        routes_to_root_help: true,
    },
];

const FLAGS: &[FlagDescriptor] = &[
    FlagDescriptor {
        id: "league",
        short: 'l',
        long: "league",
        display_label: "-l, --league <key>",
        description: "Yahoo league key",
        action: FlagAction::Value,
        routing_aliases: &[],
    },
    FlagDescriptor {
        id: "debug",
        short: 'd',
        long: "debug",
        display_label: "-d, --debug",
        description: "Print operation diagnostics",
        action: FlagAction::SetTrue,
        routing_aliases: &[],
    },
    FlagDescriptor {
        id: "version",
        short: 'v',
        long: "version",
        display_label: "-v, --version",
        description: "Print version",
        action: FlagAction::Version,
        routing_aliases: &[],
    },
    FlagDescriptor {
        id: "help",
        short: 'h',
        long: "help",
        display_label: "-h, -?, --help",
        description: "Print this help",
        action: FlagAction::Help,
        routing_aliases: &["-?"],
    },
];

/// Run the b9 command using process arguments.
pub fn run(version: &'static str) -> ExitCode {
    run_from(std::env::args_os(), version)
}

/// Run the b9 command using an injectable argument sequence.
pub fn run_from<I, T>(arguments: I, version: &'static str) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let arguments: Vec<OsString> = arguments.into_iter().map(Into::into).collect();
    if is_root_help_invocation(&arguments) {
        print!("{}", render_root_help(version, detected_help_color_mode()));
        return ExitCode::SUCCESS;
    }

    let command = root_command(version);
    let matches = match command.try_get_matches_from(arguments) {
        Ok(matches) => matches,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayVersion | ErrorKind::DisplayHelp
            ) =>
        {
            print!("{error}");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprint!("{error}");
            return ExitCode::from(2);
        }
    };

    if matches.get_flag("debug") {
        let command = matches.subcommand_name().unwrap_or("help");
        let league = if matches.get_one::<String>("league").is_some() {
            "override"
        } else {
            "saved"
        };
        eprintln!("b9 debug: command={command} league_source={league}");
    }

    match matches.subcommand() {
        Some(("i", matches)) => run_glossary(matches.get_one::<String>("term")),
        Some(("login", _)) => run_result(crate::sync::login().map(|()| String::new()), false),
        Some(("logout", _)) => run_result(crate::sync::logout(), false),
        Some(("st", _)) => run_result(
            crate::sync::status(matches.get_one::<String>("league").map(String::as_str)),
            true,
        ),
        Some(("sync", _)) => run_result(
            crate::sync::synchronize(matches.get_one::<String>("league").map(String::as_str)),
            true,
        ),
        Some(("m", subcommand)) => run_result(
            crate::matchup::show(
                matches.get_one::<String>("league").map(String::as_str),
                subcommand.get_one::<i32>("week").copied(),
            ),
            true,
        ),
        Some(("t", subcommand)) => run_result(
            crate::mlb_commands::show_teams(
                subcommand.get_one::<String>("team").map(String::as_str),
                subcommand.get_flag("force"),
            ),
            false,
        ),
        Some(("tt", subcommand)) => run_result(
            crate::mlb_commands::show_totals(subcommand.get_flag("force")),
            false,
        ),
        Some(("sp", subcommand)) => run_result(
            crate::mlb_commands::show_probables(subcommand.get_flag("force")),
            false,
        ),
        Some(("r", subcommand)) => run_result(
            crate::player_commands::show_roster(
                subcommand.get_one::<String>("team").map(String::as_str),
            ),
            false,
        ),
        Some(("rt", subcommand)) => run_result(
            crate::player_commands::show_totals(
                subcommand.get_one::<String>("weekly").map(String::as_str),
            ),
            false,
        ),
        Some(("h", subcommand)) => run_result(
            crate::player_commands::show_pool(
                "B",
                subcommand.get_one::<String>("player").map(String::as_str),
                subcommand.get_one::<String>("sort").map(String::as_str),
                subcommand.get_one::<String>("position").map(String::as_str),
                subcommand.get_flag("waiver"),
            ),
            false,
        ),
        Some(("p", subcommand)) => run_result(
            crate::player_commands::show_pool(
                "P",
                subcommand.get_one::<String>("player").map(String::as_str),
                subcommand.get_one::<String>("sort").map(String::as_str),
                subcommand.get_one::<String>("position").map(String::as_str),
                subcommand.get_flag("waiver"),
            ),
            false,
        ),
        _ => ExitCode::SUCCESS,
    }
}

fn is_root_help_invocation(arguments: &[OsString]) -> bool {
    if arguments.len() == 1 {
        return true;
    }
    let Some(token) = arguments.get(1).and_then(|argument| argument.to_str()) else {
        return false;
    };
    if arguments.len() != 2 {
        return false;
    }
    COMMANDS
        .iter()
        .any(|descriptor| descriptor.routes_to_root_help && descriptor.name == token)
        || FLAGS.iter().any(|descriptor| {
            matches!(descriptor.action, FlagAction::Help)
                && ((token.len() == 2
                    && token.starts_with('-')
                    && token.ends_with(descriptor.short))
                    || token.strip_prefix("--") == Some(descriptor.long)
                    || descriptor.routing_aliases.contains(&token))
        })
}

fn root_command(version: &'static str) -> Command {
    let mut command = Command::new("b9")
        .about("Fantasy Baseball Advisor")
        .version(version)
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .disable_version_flag(true);
    for descriptor in COMMANDS {
        let mut subcommand = Command::new(descriptor.name).about(descriptor.description);
        for alias in descriptor.aliases {
            subcommand = subcommand.alias(alias);
        }
        if let Some(argument) = descriptor.argument {
            subcommand = subcommand.arg(
                Arg::new(argument.id)
                    .value_name(argument.value_name)
                    .num_args(0..=1),
            );
        }
        if descriptor.name == "m" {
            subcommand = subcommand.arg(
                Arg::new("week")
                    .short('w')
                    .long("week")
                    .value_name("WEEK")
                    .value_parser(clap::value_parser!(i32)),
            );
        }
        if descriptor.name == "rt" {
            subcommand = subcommand.arg(
                Arg::new("weekly")
                    .short('w')
                    .long("weekly")
                    .value_name("WEEK|DATE")
                    .num_args(0..=1)
                    .default_missing_value("true"),
            );
        }
        if matches!(descriptor.name, "h" | "p") {
            subcommand = subcommand
                .arg(Arg::new("sort").short('s').long("sort").value_name("FIELD"))
                .arg(
                    Arg::new("position")
                        .short('p')
                        .long("position")
                        .value_name("POS"),
                )
                .arg(
                    Arg::new("waiver")
                        .short('w')
                        .long("waiver")
                        .action(ArgAction::SetTrue),
                );
        }
        if matches!(descriptor.name, "t" | "tt" | "sp") {
            subcommand = subcommand.arg(
                Arg::new("force")
                    .short('f')
                    .long("force")
                    .help("Refresh provider data")
                    .action(ArgAction::SetTrue),
            );
        }
        if matches!(
            descriptor.name,
            "login" | "logout" | "st" | "sync" | "m" | "t" | "tt" | "sp" | "r" | "rt" | "h" | "p"
        ) {
            subcommand = subcommand.arg(
                Arg::new("command_help")
                    .short('h')
                    .long("help")
                    .action(ArgAction::Help),
            );
        }
        command = command.subcommand(subcommand);
    }
    for descriptor in FLAGS {
        let action = match descriptor.action {
            FlagAction::Help => ArgAction::Help,
            FlagAction::Version => ArgAction::Version,
            FlagAction::SetTrue => ArgAction::SetTrue,
            FlagAction::Value => ArgAction::Set,
        };
        let mut argument = Arg::new(descriptor.id)
            .short(descriptor.short)
            .long(descriptor.long)
            .help(descriptor.description)
            .action(action);
        if matches!(descriptor.action, FlagAction::SetTrue | FlagAction::Value) {
            argument = argument.global(true);
        }
        if matches!(descriptor.action, FlagAction::Value) {
            argument = argument.value_name("KEY");
        }
        command = command.arg(argument);
    }
    command
}

fn run_result<E: std::fmt::Display>(
    result: Result<String, E>,
    yahoo_attribution: bool,
) -> ExitCode {
    match result {
        Ok(output) => {
            print!("{output}");
            if yahoo_attribution {
                eprintln!("Data provided by Yahoo Fantasy Sports.");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

/// Render root help from the same descriptors used to build the parser.
pub fn render_root_help(version: &str, mode: HelpColorMode) -> String {
    let mut output = String::new();
    output.push_str(&title("b9", mode));
    output.push_str(" v");
    output.push_str(version);
    output.push('\n');
    output.push_str(&subtitle("Fantasy Baseball Advisor", mode));
    output.push_str("\n\n");
    output.push_str(&section("USAGE", mode));
    output.push_str("\n  b9 <command> [flags]\n\n");
    output.push_str(&section("COMMANDS", mode));
    output.push('\n');
    for descriptor in COMMANDS {
        push_help_row(
            &mut output,
            descriptor.display_label,
            descriptor.description,
        );
    }
    output.push('\n');
    output.push_str(&section("FLAGS", mode));
    output.push('\n');
    for descriptor in FLAGS {
        push_help_row(
            &mut output,
            descriptor.display_label,
            descriptor.description,
        );
    }
    output
}

fn push_help_row(output: &mut String, label: &str, description: &str) {
    output.push_str(&format!("  {label:<28} {description}\n"));
}

fn run_glossary(term: Option<&String>) -> ExitCode {
    let entries = match embedded_entries() {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("i: load embedded glossary: {error}; reinstall b9");
            return ExitCode::from(1);
        }
    };
    let Some(term) = term else {
        println!("{}", render_full(&entries));
        return ExitCode::SUCCESS;
    };
    let term = term.trim();
    if term.is_empty() {
        eprintln!("i: empty term; provide a glossary key or omit TERM for the full glossary");
        return ExitCode::from(1);
    }
    match lookup(&entries, term) {
        LookupResult::Match(entry) => {
            println!("{}", render_entry(entry));
            ExitCode::SUCCESS
        }
        LookupResult::Ambiguous(entries) => {
            let keys = entries
                .iter()
                .map(|entry| entry.key.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!("i: term {term:?} is ambiguous; matches: {keys}; retry with an exact key");
            ExitCode::from(1)
        }
        LookupResult::Miss(suggestions) => {
            eprintln!(
                "i: no glossary entry matches {term:?}; closest keys: {}",
                suggestions.join(", ")
            );
            ExitCode::from(1)
        }
    }
}
