//! Terminal-aware styling for deterministic CLI presentation.

use std::io::IsTerminal;

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

fn style(value: &str, code: &str, mode: HelpColorMode) -> String {
    match mode {
        HelpColorMode::Plain => value.to_owned(),
        HelpColorMode::Color => format!("\u{1b}[{code}m{value}\u{1b}[0m"),
    }
}
