//! Terminal-aware styling for deterministic CLI presentation.

use std::io::IsTerminal;
use std::io::{self, Write};
use std::process::Command;

/// Explicit help color mode used by renderers and deterministic tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelpColorMode {
    Plain,
    Color,
}

/// Inputs that determine whether 256-color help output is safe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorContext<'a> {
    pub stdout_is_terminal: bool,
    pub no_color: Option<&'a str>,
    pub term: Option<&'a str>,
    pub colorterm: Option<&'a str>,
}

/// Resolve the help color mode from terminal and environment evidence.
pub fn help_color_mode(context: ColorContext<'_>) -> HelpColorMode {
    let disabled = !context.stdout_is_terminal
        || context.no_color.is_some_and(|value| !value.is_empty())
        || context.term == Some("dumb");
    let advertised = matches!(context.colorterm, Some("truecolor" | "24bit"))
        || context.term.is_some_and(|value| value.contains("256color"));
    if !disabled && advertised {
        HelpColorMode::Color
    } else {
        HelpColorMode::Plain
    }
}

/// Detect the process help color mode without retaining global state.
pub fn detected_help_color_mode() -> HelpColorMode {
    let no_color = std::env::var("NO_COLOR").ok();
    let term = std::env::var("TERM").ok();
    let colorterm = std::env::var("COLORTERM").ok();
    help_color_mode(ColorContext {
        stdout_is_terminal: std::io::stdout().is_terminal(),
        no_color: no_color.as_deref(),
        term: term.as_deref(),
        colorterm: colorterm.as_deref(),
    })
}

/// Read one credential from an interactive terminal with echo disabled.
pub fn read_secret(prompt: &str) -> io::Result<String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other("interactive terminal required"));
    }
    print!("{prompt}");
    io::stdout().flush()?;
    let disabled = Command::new("stty").arg("-echo").status()?.success();
    if !disabled {
        return Err(io::Error::other("disable terminal echo failed"));
    }
    let mut value = String::new();
    let read = io::stdin().read_line(&mut value);
    let restored = Command::new("stty").arg("echo").status();
    println!();
    read?;
    if !restored?.success() {
        return Err(io::Error::other("restore terminal echo failed"));
    }
    Ok(value.trim().to_owned())
}

/// Style the b9 title using its bold bright-white role.
pub fn title(value: &str, mode: HelpColorMode) -> String {
    style(value, "1;38;5;231", mode)
}

/// Style the help subtitle using b9's gray role.
pub fn subtitle(value: &str, mode: HelpColorMode) -> String {
    style(value, "38;5;245", mode)
}

/// Style a help section heading using b9's white role.
pub fn section(value: &str, mode: HelpColorMode) -> String {
    style(value, "38;5;255", mode)
}

/// Return the visible width of a string containing ANSI SGR sequences.
pub fn visible_width(value: &str) -> usize {
    let mut escape = false;
    value
        .chars()
        .filter(|character| {
            if *character == '\u{1b}' {
                escape = true;
                return false;
            }
            if escape {
                if *character == 'm' {
                    escape = false;
                }
                return false;
            }
            true
        })
        .count()
}

/// Style an MLB roster status with a shared semantic role.
pub fn roster_status(value: &str, mode: HelpColorMode) -> String {
    let code = if value.starts_with('D') {
        "38;5;178"
    } else if matches!(value, "MIN" | "RM") {
        "38;5;240"
    } else {
        "38;5;255"
    };
    style(value, code, mode)
}

/// Style secondary MLB context using the shared dark-gray role.
pub fn dim(value: &str, mode: HelpColorMode) -> String {
    style(value, "38;5;240", mode)
}

/// Style favorable or available MLB context using the shared dark-green role.
pub fn good(value: &str, mode: HelpColorMode) -> String {
    style(value, "38;5;28", mode)
}

/// Style warnings or current-roster context using the shared yellow role.
pub fn warning(value: &str, mode: HelpColorMode) -> String {
    style(value, "38;5;178", mode)
}

/// Apply the active, injured-list, or off-active semantic tier to a complete row.
pub fn roster_row(value: &str, status: &str, mode: HelpColorMode) -> String {
    if status.starts_with('D') {
        warning(value, mode)
    } else if !status.is_empty() && status != "A" {
        dim(value, mode)
    } else {
        value.to_owned()
    }
}

fn style(value: &str, code: &str, mode: HelpColorMode) -> String {
    match mode {
        HelpColorMode::Plain => value.to_owned(),
        HelpColorMode::Color => format!("\u{1b}[{code}m{value}\u{1b}[0m"),
    }
}
