use std::io::Cursor;

use b9::domain::{
    FantasyPlayer, FantasyRosterSlot, FantasyTeam, League, Matchup, Position, RosterWeekStats,
    ScoringType,
};
use b9::providers::yahoo_fantasy::{
    LeagueRosters, LeagueSettings, RosterPosition, StatCategory, UserLeague, YahooFantasyError,
    YahooFantasySource,
};
use b9::store::{Store, SyncMode, SyncOrigin, inspect_status_at};
use b9::sync::{select_league, synchronize_with, synchronize_with_origin};
use tempfile::tempdir;

struct Source {
    fail_rosters: bool,
}

impl YahooFantasySource for Source {
    fn user_leagues(&self) -> Result<Vec<UserLeague>, YahooFantasyError> {
        Ok(leagues())
    }

    fn team_key(&self, _: &str) -> Result<String, YahooFantasyError> {
        Ok("mlb.l.1.t.1".into())
    }

    fn league_settings(&self, _: &str) -> Result<LeagueSettings, YahooFantasyError> {
        Ok(LeagueSettings {
            league: League {
                league_key: "mlb.l.1".into(),
                name: "League".into(),
                season: 2026,
                num_teams: 2,
                scoring_type: ScoringType::HeadToHead,
                roster_positions: vec![Position::Outfield],
                batting_categories: vec!["R".into()],
                pitching_categories: vec![],
            },
            current_week: Some(7),
            categories: vec![StatCategory {
                stat_id: 7,
                abbreviation: "R".into(),
                name: "Runs".into(),
                sort_order: 1,
                display_only: false,
                sequence: 0,
            }],
            roster_positions: vec![RosterPosition {
                position: Position::Outfield,
                count: 1,
            }],
        })
    }

    fn standings(&self, _: &str) -> Result<Vec<FantasyTeam>, YahooFantasyError> {
        Ok(vec![team(1), team(2)])
    }

    fn league_rosters(&self, _: &str) -> Result<LeagueRosters, YahooFantasyError> {
        if self.fail_rosters {
            return Err(YahooFantasyError::Incomplete("rosters are incomplete"));
        }
        Ok(LeagueRosters {
            players: vec![player(101, "Ada Hitter"), player(102, "Grace Hitter")],
            slots: vec![slot(1, 101), slot(2, 102)],
        })
    }

    fn free_agents(&self, _: &str) -> Result<Vec<FantasyPlayer>, YahooFantasyError> {
        Ok(Vec::new())
    }

    fn scoreboard(&self, _: &str, _: Option<i32>) -> Result<Vec<Matchup>, YahooFantasyError> {
        unreachable!()
    }

    fn roster_week_stats(&self, _: &str, _: i32) -> Result<RosterWeekStats, YahooFantasyError> {
        unreachable!()
    }
}

fn leagues() -> Vec<UserLeague> {
    vec![
        UserLeague {
            league_key: "mlb.l.1".into(),
            name: "Alpha".into(),
            season: 2026,
            team_key: "mlb.l.1.t.1".into(),
            team_name: "One".into(),
        },
        UserLeague {
            league_key: "mlb.l.2".into(),
            name: "Beta".into(),
            season: 2026,
            team_key: "mlb.l.2.t.1".into(),
            team_name: "Other".into(),
        },
    ]
}

fn team(id: i64) -> FantasyTeam {
    FantasyTeam {
        team_key: format!("mlb.l.1.t.{id}"),
        league_key: "mlb.l.1".into(),
        team_id: id,
        name: format!("Team {id}"),
        manager_name: format!("Manager {id}"),
        is_owned_by_current_login: id == 1,
        waiver_priority: id,
        faab_balance: 100 - id,
        wins: 10 - id,
        losses: id,
        ties: 1,
        moves: id * 2,
        rank: id,
    }
}

fn player(id: i64, name: &str) -> FantasyPlayer {
    FantasyPlayer {
        yahoo_player_id: id,
        name: name.into(),
        mlb_team: "NYY".into(),
        display_position: "OF".into(),
        position_type: "B".into(),
        eligible_positions: vec![Position::Outfield],
        injury_status: String::new(),
        percent_owned: Some(90.0),
        yahoo_rank: Some(id),
    }
}

fn slot(team_id: i64, player_id: i64) -> FantasyRosterSlot {
    FantasyRosterSlot {
        team_key: format!("mlb.l.1.t.{team_id}"),
        yahoo_player_id: player_id,
        slot_position: Position::Outfield,
    }
}

#[test]
fn league_selection_handles_interactive_and_noninteractive_ambiguity() {
    let values = leagues();
    let mut output = Vec::new();
    let selected =
        select_league(&values, None, true, &mut Cursor::new("2\n"), &mut output).unwrap();
    assert_eq!(selected.as_deref(), Some("mlb.l.2"));
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "Select a league:\n  1. mlb.l.1  Alpha\n  2. mlb.l.2  Beta\nChoice: "
    );
    assert!(select_league(&values, None, false, &mut Cursor::new(""), &mut Vec::new()).is_err());
    assert!(
        select_league(
            &values,
            Some("unknown"),
            false,
            &mut Cursor::new(""),
            &mut Vec::new()
        )
        .is_err()
    );
}

#[test]
fn synchronization_is_complete_and_retains_prior_rows_on_fetch_failure() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
    let mut identities = |_| Vec::new();
    let summary = synchronize_with(
        &Source {
            fail_rosters: false,
        },
        &mut store,
        "mlb.l.1",
        &mut identities,
    )
    .unwrap();
    assert_eq!(
        (summary.teams, summary.players, summary.roster_slots),
        (2, 2, 2)
    );
    assert_eq!(store.fantasy_teams("mlb.l.1").unwrap().len(), 2);
    let status = inspect_status_at(store.path(), "mlb.l.1").unwrap();
    assert_eq!(status.latest_sync_status.as_deref(), Some("complete"));
    assert!(status.league_synced_at.is_some());
    assert!(
        synchronize_with(
            &Source { fail_rosters: true },
            &mut store,
            "mlb.l.1",
            &mut identities
        )
        .is_err()
    );
    assert_eq!(store.fantasy_teams("mlb.l.1").unwrap().len(), 2);
}

#[test]
fn application_boundaries_remain_layered() {
    let cli = include_str!("../src/cli.rs");
    let matchup = include_str!("../src/matchup.rs");
    let yahoo = include_str!("../src/providers/yahoo_fantasy.rs");
    assert!(!cli.contains("rusqlite"));
    assert!(!cli.contains("YahooClient"));
    assert!(!matchup.contains("rusqlite"));
    assert!(!yahoo.contains("crate::store"));
}

#[test]
fn all_callers_record_the_shared_synchronization_service_origin() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("b9.db")).unwrap();
    let mut identities = |_| Vec::new();
    for origin in [
        SyncOrigin::Manual,
        SyncOrigin::Startup,
        SyncOrigin::Automatic,
    ] {
        synchronize_with_origin(
            &Source {
                fail_rosters: false,
            },
            &mut store,
            "mlb.l.1",
            origin,
            &mut identities,
        )
        .unwrap();
        assert_eq!(
            store
                .latest_sync_run(SyncMode::Live)
                .unwrap()
                .unwrap()
                .origin,
            origin
        );
    }
}
