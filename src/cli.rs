//! Root command metadata, parsing, dispatch, streams, and exit behavior.

use std::ffi::OsString;
use std::process::ExitCode;

use clap::{Arg, ArgAction, Command, error::ErrorKind};

use crate::glossary::{LookupResult, embedded_entries, lookup, render_entry, render_full};

const ROOT_HELP_TEMPLATE: &str =
    "Usage: {usage}\n\n{about}\n\nCommands:\n{subcommands}\n\nOptions:\n{options}";

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
    let arguments: Vec<OsString> = arguments
        .into_iter()
        .map(|argument| {
            let argument = argument.into();
            if argument == "-?" {
                OsString::from("--help")
            } else {
                argument
            }
        })
        .collect();
    let mut command = root_command(version);
    if arguments.len() == 1 {
        command.print_help().expect("write root help");
        return ExitCode::SUCCESS;
    }

    let matches = match command.try_get_matches_from(arguments) {
        Ok(matches) => matches,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
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

    match matches.subcommand() {
        Some(("whatis", matches)) => run_glossary(matches.get_one::<String>("term")),
        Some(("help", _)) => {
            let mut command = root_command(version);
            command.print_help().expect("write root help");
            ExitCode::SUCCESS
        }
        _ => ExitCode::SUCCESS,
    }
}

fn root_command(version: &'static str) -> Command {
    Command::new("b9")
        .about("Fantasy Baseball Advisor")
        .version(version)
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .disable_version_flag(true)
        .override_usage("b9 [OPTIONS] [COMMAND]")
        .help_template(ROOT_HELP_TEMPLATE)
        .subcommand(
            Command::new("whatis")
                .about("Look up a term in the b9 glossary")
                .visible_alias("i")
                .display_order(1)
                .arg(Arg::new("term").value_name("TERM").num_args(0..=1)),
        )
        .subcommand(Command::new("help").about("Print help").display_order(2))
        .arg(
            Arg::new("help")
                .short('h')
                .long("help")
                .help("Print help")
                .action(ArgAction::Help)
                .display_order(1),
        )
        .arg(
            Arg::new("version")
                .short('v')
                .long("version")
                .help("Print version")
                .action(ArgAction::Version)
                .display_order(2),
        )
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
