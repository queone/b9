use std::collections::HashMap;
use std::io::Cursor;

use skout::domain::{
    FantasyPlayer, FantasyRosterSlot, FantasyTeam, League, Matchup, MatchupTeam, PlayerWeekStats,
    Position, RosterWeekStats, ScoringType,
};
use skout::providers::yahoo_fantasy::{
    LeagueRosters, LeagueSettings, RosterPosition, StatCategory, YahooFantasyError,
    YahooFantasySource,
};
use skout::store::{
    FantasySnapshotWrite, PositionWrite, Store, SyncMode, SyncOrigin, inspect_status_at,
};
use skout::sync::{select_primary_team, synchronize_with, synchronize_with_origin};
use tempfile::tempdir;

struct Source {
    fail_rosters: bool,
}

impl YahooFantasySource for Source {
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
        Ok(vec![player(103, "Katherine Free Agent")])
    }

    fn scoreboard(&self, _: &str, week: Option<i32>) -> Result<Vec<Matchup>, YahooFantasyError> {
        Ok(vec![matchup(week.unwrap_or(7))])
    }

    fn roster_week_stats(
        &self,
        team_key: &str,
        week: i32,
    ) -> Result<RosterWeekStats, YahooFantasyError> {
        Ok(RosterWeekStats {
            team_key: team_key.into(),
            team_name: team_key.into(),
            week,
            players: Vec::<PlayerWeekStats>::new(),
        })
    }
}

fn matchup(week: i32) -> Matchup {
    let matchup_team = |id| MatchupTeam {
        team_key: format!("mlb.l.1.t.{id}"),
        team_id: id,
        name: format!("Team {id}"),
        is_current_login: id == 1,
        stats: HashMap::new(),
        wins: 0,
        losses: 0,
        ties: 0,
        completed_games: 0,
        live_games: 0,
        remaining_games: 0,
    };
    Matchup {
        week,
        week_start: "2026-04-01".into(),
        week_end: "2026-04-07".into(),
        status: "postevent".into(),
        teams: [matchup_team(1), matchup_team(2)],
    }
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
        percentage_started: Some(80.0),
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
fn primary_team_selection_handles_matching_prompt_and_ambiguity() {
    let values = vec![team(1), team(2)];
    let mut output = Vec::new();
    let selected =
        select_primary_team(&values, None, true, &mut Cursor::new("2\n"), &mut output).unwrap();
    assert_eq!(selected, "mlb.l.1.t.2");
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "Select your primary team:\n  1. mlb.l.1.t.1  Team 1\n  2. mlb.l.1.t.2  Team 2\nChoice: "
    );
    assert_eq!(
        select_primary_team(
            &values,
            Some("MLB.L.1.T.1"),
            false,
            &mut Cursor::new(""),
            &mut Vec::new(),
        )
        .unwrap_err()
        .to_string(),
        "select primary team: no team matches \"MLB.L.1.T.1\"; run skout sync -T <key-or-name> and retry"
    );
    assert_eq!(
        select_primary_team(
            &values,
            Some("team 1"),
            false,
            &mut Cursor::new(""),
            &mut Vec::new(),
        )
        .unwrap(),
        "mlb.l.1.t.1"
    );
    assert!(
        select_primary_team(
            &values,
            Some("Team"),
            false,
            &mut Cursor::new(""),
            &mut Vec::new()
        )
        .is_err()
    );
    let mut output = Vec::new();
    assert_eq!(
        select_primary_team(
            &values,
            Some("stale team"),
            true,
            &mut Cursor::new("1\n"),
            &mut output,
        )
        .unwrap(),
        "mlb.l.1.t.1"
    );
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("Select your primary team:")
    );
}

#[test]
fn synchronization_is_complete_and_retains_prior_rows_on_fetch_failure() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("skout.db")).unwrap();
    let mut identities = |_| Vec::new();
    let summary = synchronize_with(
        &Source {
            fail_rosters: false,
        },
        &mut store,
        "mlb.l.1",
        "mlb.l.1.t.1",
        &mut identities,
    )
    .unwrap();
    assert_eq!(
        (summary.teams, summary.players, summary.roster_slots),
        (2, 3, 2)
    );
    assert_eq!(store.fantasy_teams("mlb.l.1").unwrap().len(), 2);
    let prior_players = store.fantasy_players("mlb.l.1").unwrap();
    assert_eq!(prior_players.len(), 3);
    assert_eq!(
        prior_players
            .iter()
            .find(|player| player.yahoo_player_id == Some(103))
            .unwrap()
            .owner,
        None
    );
    assert!(
        store
            .command_snapshot("match_scoreboard", "yahoo", "mlb.l.1:1")
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .command_snapshot("match_roster", "yahoo", "mlb.l.1.t.2:7")
            .unwrap()
            .is_some()
    );
    let status = inspect_status_at(store.path(), "mlb.l.1").unwrap();
    assert_eq!(status.latest_sync_status.as_deref(), Some("complete"));
    assert!(status.league_synced_at.is_some());
    assert!(
        synchronize_with(
            &Source { fail_rosters: true },
            &mut store,
            "mlb.l.1",
            "mlb.l.1.t.1",
            &mut identities
        )
        .is_err()
    );
    assert_eq!(store.fantasy_teams("mlb.l.1").unwrap().len(), 2);
    assert_eq!(store.fantasy_players("mlb.l.1").unwrap(), prior_players);
}

#[test]
fn public_merge_updates_team_transactions_and_preserves_other_supplements() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("skout.db")).unwrap();
    let source = Source {
        fail_rosters: false,
    };
    let mut identities = |_| Vec::new();
    synchronize_with(
        &source,
        &mut store,
        "mlb.l.1",
        "mlb.l.1.t.1",
        &mut identities,
    )
    .unwrap();

    let settings = source.league_settings("mlb.l.1").unwrap();
    let mut public_teams = source.standings("mlb.l.1").unwrap();
    public_teams[0].waiver_priority = 4;
    public_teams[0].faab_balance = 65;
    public_teams[0].moves = 30;
    let mut public_players = source.league_rosters("mlb.l.1").unwrap().players;
    let target_id = public_players[0].yahoo_player_id;
    let mut precise = public_players[0].clone();
    precise.injury_status = "IL60".into();
    store.merge_fantasy_players(&[precise]).unwrap();
    for player in &mut public_players {
        player.percent_owned = None;
        player.yahoo_rank = None;
    }
    public_players[0].injury_status = "IL".into();
    store
        .merge_public_fantasy_snapshot(&FantasySnapshotWrite {
            league: settings.league,
            current_week: settings.current_week,
            categories: Vec::new(),
            positions: vec![PositionWrite {
                position: "OF".into(),
                count: 1,
            }],
            teams: public_teams,
            players: public_players.clone(),
            slots: source.league_rosters("mlb.l.1").unwrap().slots,
        })
        .unwrap();

    let teams = store.fantasy_teams("mlb.l.1").unwrap();
    assert_eq!(
        (
            teams[0].waiver_priority,
            teams[0].faab_balance,
            teams[0].moves
        ),
        (4, 65, 30)
    );
    let players = store.fantasy_players("mlb.l.1").unwrap();
    assert_eq!(players[0].rank, Some(101));
    assert_eq!(players[0].percent_owned, Some(90.0));
    assert_eq!(
        players
            .iter()
            .find(|player| player.yahoo_player_id == Some(target_id))
            .unwrap()
            .status,
        "IL60"
    );

    let mut active = public_players[0].clone();
    active.injury_status.clear();
    store.merge_fantasy_players(&[active]).unwrap();
    let players = store.fantasy_players("mlb.l.1").unwrap();
    assert!(
        players
            .iter()
            .find(|player| player.yahoo_player_id == Some(target_id))
            .unwrap()
            .status
            .is_empty()
    );

    let mut dtd = public_players[0].clone();
    dtd.injury_status = "DTD".into();
    store.merge_fantasy_players(&[dtd]).unwrap();
    let settings = source.league_settings("mlb.l.1").unwrap();
    store
        .merge_public_fantasy_snapshot(&FantasySnapshotWrite {
            league: settings.league,
            current_week: settings.current_week,
            categories: Vec::new(),
            positions: vec![PositionWrite {
                position: "OF".into(),
                count: 1,
            }],
            teams: source.standings("mlb.l.1").unwrap(),
            players: public_players,
            slots: source.league_rosters("mlb.l.1").unwrap().slots,
        })
        .unwrap();
    assert_eq!(
        store
            .fantasy_players("mlb.l.1")
            .unwrap()
            .into_iter()
            .find(|player| player.yahoo_player_id == Some(target_id))
            .unwrap()
            .status,
        "DTD"
    );
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
fn circuit_opens_after_five_failures_and_closes_on_recovery() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("skout.db")).unwrap();
    let mut identities = |_| Vec::new();
    for _ in 0..5 {
        assert!(
            synchronize_with_origin(
                &Source { fail_rosters: true },
                &mut store,
                "mlb.l.1",
                "mlb.l.1.t.1",
                SyncOrigin::Manual,
                &mut identities,
            )
            .is_err()
        );
    }
    let opened = store.dashboard_status().unwrap();
    assert!(opened.circuit_open);
    assert_eq!(opened.provider_failure_count, 5);
    synchronize_with_origin(
        &Source {
            fail_rosters: false,
        },
        &mut store,
        "mlb.l.1",
        "mlb.l.1.t.1",
        SyncOrigin::Manual,
        &mut identities,
    )
    .unwrap();
    let recovered = store.dashboard_status().unwrap();
    assert!(!recovered.circuit_open);
    assert_eq!(recovered.provider_failure_count, 0);
}

#[test]
fn local_status_reports_real_identities_once_a_league_has_synced() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("skout.db")).unwrap();
    let mut identities = |_| Vec::new();
    let before = inspect_status_at(store.path(), "mlb.l.1").unwrap();
    assert_eq!(before.yahoo_identity_count, 0);
    assert_eq!(before.mlb_identity_count, 0);
    synchronize_with(
        &Source {
            fail_rosters: false,
        },
        &mut store,
        "mlb.l.1",
        "mlb.l.1.t.1",
        &mut identities,
    )
    .unwrap();
    let after = inspect_status_at(store.path(), "mlb.l.1").unwrap();
    assert_eq!(after.yahoo_identity_count, 3);
    assert_eq!(after.unmatched_player_count, 3);
}

#[test]
fn injected_sync_origins_remain_durably_decodable() {
    let directory = tempdir().unwrap();
    let mut store = Store::open_at(directory.path().join("skout.db")).unwrap();
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
            "mlb.l.1.t.1",
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
