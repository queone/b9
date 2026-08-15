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
struct AliasDescriptor {
    name: &'static str,
    display_label: &'static str,
    description: &'static str,
}

#[derive(Clone, Copy)]
struct CommandDescriptor {
    name: &'static str,
    display_label: &'static str,
    description: &'static str,
    argument: Option<ArgumentDescriptor>,
    aliases: &'static [AliasDescriptor],
    routes_to_root_help: bool,
}

#[derive(Clone, Copy)]
enum FlagAction {
    Help,
    Version,
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

const WHATIS_ALIASES: &[AliasDescriptor] = &[AliasDescriptor {
    name: "i",
    display_label: "i [term]",
    description: "Alias for whatis",
}];

const COMMANDS: &[CommandDescriptor] = &[
    CommandDescriptor {
        name: "whatis",
        display_label: "whatis [term]",
        description: "Look up a term in the b9 glossary",
        argument: Some(ArgumentDescriptor {
            id: "term",
            value_name: "TERM",
        }),
        aliases: WHATIS_ALIASES,
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
        Err(error) if matches!(error.kind(), ErrorKind::DisplayVersion) => {
            print!("{error}");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprint!("{error}");
            return ExitCode::from(2);
        }
    };

    match matches.subcommand() {
        Some(("whatis", matches)) => run_glossary(matches.get_one::<String>("term")),
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
            subcommand = subcommand.visible_alias(alias.name);
        }
        if let Some(argument) = descriptor.argument {
            subcommand = subcommand.arg(
                Arg::new(argument.id)
                    .value_name(argument.value_name)
                    .num_args(0..=1),
            );
        }
        command = command.subcommand(subcommand);
    }
    for descriptor in FLAGS {
        let action = match descriptor.action {
            FlagAction::Help => ArgAction::Help,
            FlagAction::Version => ArgAction::Version,
        };
        command = command.arg(
            Arg::new(descriptor.id)
                .short(descriptor.short)
                .long(descriptor.long)
                .help(descriptor.description)
                .action(action),
        );
    }
    command
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
        for alias in descriptor.aliases {
            push_help_row(&mut output, alias.display_label, alias.description);
        }
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
            eprintln!("whatis: load embedded glossary: {error}; reinstall b9");
            return ExitCode::from(1);
        }
    };
    let Some(term) = term else {
        println!("{}", render_full(&entries));
        return ExitCode::SUCCESS;
    };
    let term = term.trim();
    if term.is_empty() {
        eprintln!("whatis: empty term; provide a glossary key or omit TERM for the full glossary");
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
            eprintln!(
                "whatis: term {term:?} is ambiguous; matches: {keys}; retry with an exact key"
            );
            ExitCode::from(1)
        }
        LookupResult::Miss(suggestions) => {
            eprintln!(
                "whatis: no glossary entry matches {term:?}; closest keys: {}",
                suggestions.join(", ")
            );
            ExitCode::from(1)
        }
    }
}
