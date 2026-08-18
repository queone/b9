use b9::domain::{
    BattingStats, MlbRosterPlayer, MlbSlateRow, MlbStanding, MlbTeam, MlbTeamTotals, PitchingStats,
};
use b9::mlb_display::{render_rosters, render_slate, render_totals};
use b9::terminal::HelpColorMode;

#[test]
fn roster_display_separates_roles_and_labels_statuses() {
    let rows: Vec<MlbRosterPlayer> =
        serde_json::from_str(include_str!("fixtures/mlb/team-rosters.json")).unwrap();
    let output = render_rosters(
        &[("LAA - Los Angeles Angels (70-50)".into(), rows)],
        &[],
        HelpColorMode::Plain,
    );
    assert!(output.contains("HITTER"));
    assert!(output.contains("PITCHER"));
    assert!(output.contains("IL10"));
    assert!(output.contains("Final @ TEX"));
    assert_eq!(output.matches("Two Way").count(), 2);
    assert_eq!(output, include_str!("fixtures/mlb/team-roster.txt"));
}

fn yankees() -> MlbTeam {
    MlbTeam {
        id: 147,
        name: "New York Yankees".into(),
        location: "New York".into(),
        club_name: "Yankees".into(),
        abbreviation: "NYY".into(),
        league_id: 103,
    }
}

#[test]
fn totals_plain_output_matches_golden() {
    let team = yankees();
    let output = render_totals(
        &[MlbStanding {
            team: team.clone(),
            wins: 70,
            losses: 50,
            games_back: "-".into(),
        }],
        &[MlbTeamTotals {
            team,
            batting: BattingStats {
                plate_appearances: 4500,
                runs: 600,
                home_runs: 180,
                runs_batted_in: 575,
                stolen_bases: 90,
                batting_average: 0.250,
                on_base_percentage: 0.330,
                slugging_percentage: 0.440,
                on_base_plus_slugging: 0.770,
                ..Default::default()
            },
            pitching: PitchingStats {
                games: 120,
                games_started: 120,
                innings_pitched: 1080.0,
                wins: 70,
                saves: 35,
                holds: 60,
                strikeouts: 1100,
                earned_run_average: 3.75,
                whip: 1.20,
                ..Default::default()
            },
            yahoo_players: Some(14),
            players_available: Some(26),
        }],
        false,
        HelpColorMode::Plain,
    );
    assert_eq!(output, include_str!("fixtures/mlb/team-totals.txt"));
}

#[test]
fn totals_color_only_context_columns_like_skout() {
    let team = yankees();
    let output = render_totals(
        &[MlbStanding {
            team: team.clone(),
            wins: 70,
            losses: 50,
            games_back: "-".into(),
        }],
        &[MlbTeamTotals {
            team,
            batting: BattingStats {
                plate_appearances: 4500,
                runs: 600,
                ..Default::default()
            },
            pitching: PitchingStats {
                innings_pitched: 1080.0,
                quality_starts: 75,
                wins: 70,
                ..Default::default()
            },
            yahoo_players: Some(14),
            players_available: Some(26),
        }],
        false,
        HelpColorMode::Color,
    );
    assert!(output.contains("NYY   \u{1b}[38;5;245m 70\u{1b}[0m"));
    assert!(output.contains("\u{1b}[38;5;245m1080.0\u{1b}[0m"));
    assert!(output.contains("\u{1b}[38;5;245m 75\u{1b}[0m   70"));
    assert!(!output.contains("\u{1b}[38;5;245m600\u{1b}[0m"));
}

#[test]
fn probable_pitchers_plain_output_matches_golden() {
    let output = render_slate(
        &[MlbSlateRow {
            date: "2026-08-15".into(),
            game_id: 1,
            game_time: "7:05 PM EDT".into(),
            away_team: "NYY".into(),
            home_team: "BOS".into(),
            away_pitcher: "Ace Starter".into(),
            home_pitcher: "Home Pitcher".into(),
            win_probability: Some(0.60),
            away_free_agent: true,
            home_free_agent: false,
            away_mine: false,
            home_mine: true,
        }],
        &[],
        HelpColorMode::Plain,
    );
    assert_eq!(output, include_str!("fixtures/mlb/probable-pitchers.txt"));
}
