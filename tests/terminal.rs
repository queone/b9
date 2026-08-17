use b9::cli::render_root_help;
use b9::terminal::{
    ColorContext, HelpColorMode, help_color_mode, injury_status, roster_status, section, subtitle,
    title, visible_width,
};

const PLAIN_HELP: &str = "b9 v0.22.0\nFantasy Baseball advisor — github.com/queone/b9\n\nUSAGE\n  b9 <command> [flags]\n\nCOMMANDS\n  login                        Authenticate with Yahoo\n  logout                       Remove Yahoo authentication\n  st                           Show status and select a league\n  sync                         Synchronize the selected league\n  pp                           Fetch public Yahoo league data without login\n  start                        Start the background sync daemon\n  stop                         Stop the background sync daemon\n  restart                      Restart the background sync daemon\n  log                          Show or follow the daemon log\n  reset                        Delete the local b9 database\n  fetch <path>                 Perform a raw Yahoo API GET\n  lm                           Configure the advisory provider\n  m [team]                     Show a daily or weekly matchup\n  t [team]                     Show MLB 40-man rosters\n  tt                           Show MLB standings and team totals\n  sp                           Show the three-day probable-pitcher slate\n  r [name]                     Show a fantasy roster\n  rt                           Show fantasy roster totals\n  h [N|name]                   Browse hitters or show a player\n  p [N|name]                   Browse pitchers or show a player\n  i [term]                     Look up a term in the b9 glossary\n  help                         Print this help\n\nFLAGS\n  -l, --league <key>           Yahoo league key\n  -d, --debug                  Print operation diagnostics\n  -v, --version                Print version\n  -h, -?, --help               Print this help\n\nFantasy data provided by https://sports.yahoo.com/fantasy/\n";

#[test]
fn plain_help_matches_the_b9_style_golden() {
    let output = render_root_help("0.22.0", HelpColorMode::Plain);
    assert_eq!(output, PLAIN_HELP);
    assert!(!output.contains('\u{1b}'));
    assert!(!output.ends_with("\n\n"));
}

#[test]
fn colored_help_uses_only_the_contracted_spans() {
    let output = render_root_help("0.22.0", HelpColorMode::Color);
    let expected = PLAIN_HELP
        .replacen("b9", "\u{1b}[1;38;5;231mb9\u{1b}[0m", 1)
        .replacen(
            "Fantasy Baseball advisor — github.com/queone/b9",
            "\u{1b}[38;5;245mFantasy Baseball advisor — github.com/queone/b9\u{1b}[0m",
            1,
        )
        .replace("USAGE", "\u{1b}[38;5;255mUSAGE\u{1b}[0m")
        .replace("COMMANDS", "\u{1b}[38;5;255mCOMMANDS\u{1b}[0m")
        .replace("FLAGS", "\u{1b}[38;5;255mFLAGS\u{1b}[0m")
        .replace(
            "Fantasy data provided by https://sports.yahoo.com/fantasy/",
            "\u{1b}[38;5;245mFantasy data provided by https://sports.yahoo.com/fantasy/\u{1b}[0m",
        );
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
    assert_eq!(
        injury_status("IL60", HelpColorMode::Color),
        "\u{1b}[38;5;196mIL60\u{1b}[0m"
    );
    assert_eq!(injury_status("IL60", HelpColorMode::Plain), "IL60");
}

#[test]
fn dashboard_renders_settled_field_order_and_semantic_colors_within_eighty_columns() {
    use b9::config::Config;
    use b9::store::StoreStatus;
    use b9::sync::render_dashboard;
    use std::path::Path;

    let status = StoreStatus {
        mlb_identity_count: 512,
        yahoo_identity_count: 480,
        unmatched_player_count: 6,
        provider_freshness_at: Some(100),
        daemon_started_at: Some(40),
        last_run_status: Some("success".into()),
        last_run_at: Some(100),
        next_run_at: Some(200),
        database_bytes: Some(1024),
        schema_version: Some(3),
        ..StoreStatus::default()
    };
    let config = Config {
        current_league: "431.l.12345".into(),
        ..Config::default()
    };

    const PLAIN: &str = "Yahoo: not checked (run b9 login or b9 sync)\nService: running (uptime 0h 2m 0s)\nLast run: success at unix 100\nNext run: unix 200\nDatabase: /srv/b9/.config/b9/b9.db (1024 bytes, schema v3)\nIdentities: 512 MLB, 480 Yahoo\nProvider freshness: unix 100\nCircuit: closed (0 failed requests)\nLast provider error: none\nUnmatched players: 6\nLeague: 431.l.12345\nConfig: /srv/b9/.config/b9/config.json\n";

    let plain = render_dashboard(
        Path::new("/srv/b9/.config/b9/b9.db"),
        Path::new("/srv/b9/.config/b9/config.json"),
        &config,
        &status,
        160,
        HelpColorMode::Plain,
    );
    assert_eq!(plain, PLAIN);
    assert!(!plain.contains('\u{1b}'));
    for line in plain.lines() {
        assert!(visible_width(line) <= 80, "line too wide: {line}");
    }

    let colored = render_dashboard(
        Path::new("/srv/b9/.config/b9/b9.db"),
        Path::new("/srv/b9/.config/b9/config.json"),
        &config,
        &status,
        160,
        HelpColorMode::Color,
    );
    let expected = PLAIN
        .replacen(
            "running (uptime 0h 2m 0s)",
            "\u{1b}[38;5;34mrunning (uptime 0h 2m 0s)\u{1b}[0m",
            1,
        )
        .replacen(
            "success at unix 100",
            "\u{1b}[38;5;34msuccess at unix 100\u{1b}[0m",
            1,
        )
        .replacen("closed", "\u{1b}[38;5;34mclosed\u{1b}[0m", 1);
    assert_eq!(colored, expected);
    for line in colored.lines() {
        assert!(visible_width(line) <= 80, "line too wide: {line}");
    }

    let order = [
        "Service:",
        "Last run:",
        "Next run:",
        "Database:",
        "Identities:",
        "Provider freshness:",
        "Circuit:",
        "Unmatched players:",
        "League:",
    ];
    let positions: Vec<usize> = order
        .iter()
        .map(|label| plain.find(label).expect("field present"))
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn dashboard_colors_stopped_service_and_open_circuit_distinctly() {
    use b9::config::Config;
    use b9::store::StoreStatus;
    use b9::sync::render_dashboard;
    use std::path::Path;

    let status = StoreStatus {
        mlb_identity_count: 1,
        circuit_open: true,
        provider_failure_count: 5,
        provider_last_error: Some("Yahoo API returned HTTP 403".into()),
        last_run_status: Some("failed".into()),
        last_run_at: Some(50),
        ..StoreStatus::default()
    };

    let plain = render_dashboard(
        Path::new("/db"),
        Path::new("/config.json"),
        &Config::default(),
        &status,
        60,
        HelpColorMode::Plain,
    );
    assert!(plain.contains("Service: stopped"));
    assert!(plain.contains("Last run: failed at unix 50"));
    assert!(plain.contains("Circuit: open (5 failed requests)"));
    assert!(plain.contains("Last provider error: Yahoo API returned HTTP 403"));

    let colored = render_dashboard(
        Path::new("/db"),
        Path::new("/config.json"),
        &Config::default(),
        &status,
        60,
        HelpColorMode::Color,
    );
    assert!(colored.contains("\u{1b}[38;5;245mstopped\u{1b}[0m"));
    assert!(colored.contains("\u{1b}[38;5;100mfailed at unix 50\u{1b}[0m"));
    assert!(colored.contains("\u{1b}[38;5;100mopen\u{1b}[0m"));
}
