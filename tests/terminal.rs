use b9::cli::render_root_help;
use b9::terminal::{
    ColorContext, HelpColorMode, help_color_mode, roster_status, section, subtitle, title,
    visible_width,
};

const PLAIN_HELP: &str = "b9 v0.16.0\nFantasy Baseball Advisor\n\nUSAGE\n  b9 <command> [flags]\n\nCOMMANDS\n  login                        Authenticate with Yahoo\n  logout                       Remove Yahoo authentication\n  st                           Show status and select a league\n  sync                         Synchronize the selected league\n  m                            Show the baseline weekly matchup\n  t [team]                     Show MLB 40-man rosters\n  tt                           Show MLB standings and team totals\n  sp                           Show the three-day probable-pitcher slate\n  r [name]                     Show a fantasy roster\n  rt                           Show fantasy roster totals\n  h [N|name]                   Browse hitters or show a player\n  p [N|name]                   Browse pitchers or show a player\n  i [term]                     Look up a term in the b9 glossary\n  help                         Print this help\n\nFLAGS\n  -l, --league <key>           Yahoo league key\n  -d, --debug                  Print operation diagnostics\n  -v, --version                Print version\n  -h, -?, --help               Print this help\n";

#[test]
fn plain_help_matches_the_b9_style_golden() {
    let output = render_root_help("0.16.0", HelpColorMode::Plain);
    assert_eq!(output, PLAIN_HELP);
    assert!(!output.contains('\u{1b}'));
    assert!(!output.ends_with("\n\n"));
}

#[test]
fn colored_help_uses_only_the_contracted_spans() {
    let output = render_root_help("0.16.0", HelpColorMode::Color);
    let expected = PLAIN_HELP
        .replacen("b9", "\u{1b}[1;38;5;231mb9\u{1b}[0m", 1)
        .replacen(
            "Fantasy Baseball Advisor",
            "\u{1b}[38;5;245mFantasy Baseball Advisor\u{1b}[0m",
            1,
        )
        .replace("USAGE", "\u{1b}[38;5;255mUSAGE\u{1b}[0m")
        .replace("COMMANDS", "\u{1b}[38;5;255mCOMMANDS\u{1b}[0m")
        .replace("FLAGS", "\u{1b}[38;5;255mFLAGS\u{1b}[0m");
    assert_eq!(output, expected);
    assert_eq!(
        title("b9", HelpColorMode::Color),
        "\u{1b}[1;38;5;231mb9\u{1b}[0m"
    );
    assert_eq!(
        subtitle("x", HelpColorMode::Color),
        "\u{1b}[38;5;245mx\u{1b}[0m"
    );
    assert_eq!(
        section("X", HelpColorMode::Color),
        "\u{1b}[38;5;255mX\u{1b}[0m"
    );
}

fn context<'a>(
    stdout_is_terminal: bool,
    no_color: Option<&'a str>,
    term: Option<&'a str>,
    colorterm: Option<&'a str>,
) -> ColorContext<'a> {
    ColorContext {
        stdout_is_terminal,
        no_color,
        term,
        colorterm,
    }
}

#[test]
fn color_detection_requires_terminal_and_advertised_support() {
    assert_eq!(
        help_color_mode(context(true, None, Some("xterm-256color"), None)),
        HelpColorMode::Color
    );
    assert_eq!(
        help_color_mode(context(true, None, Some("xterm"), Some("truecolor"))),
        HelpColorMode::Color
    );
    assert_eq!(
        help_color_mode(context(true, Some(""), Some("xterm"), Some("24bit"))),
        HelpColorMode::Color
    );
    for disabled in [
        context(false, None, Some("xterm-256color"), None),
        context(true, Some("1"), Some("xterm-256color"), None),
        context(true, None, Some("dumb"), Some("truecolor")),
        context(true, None, Some("xterm"), None),
    ] {
        assert_eq!(help_color_mode(disabled), HelpColorMode::Plain);
    }
}

#[test]
fn mlb_status_roles_preserve_visible_width() {
    let value = roster_status("IL D10", HelpColorMode::Color);
    assert_eq!(visible_width(&value), 6);
    assert_eq!(roster_status("MINORS", HelpColorMode::Plain), "MINORS");
}
