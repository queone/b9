use std::collections::HashMap;

use b9::advisory::{
    AdvisoryAction, AdvisoryContext, AdvisoryResponse, build_advisory_context,
    compute_category_gaps, compute_lineup_candidates, compute_risk_alerts, compute_roster_moves,
    compute_slot_gaps, grounded_response,
};
use b9::domain::{FreeAgentCategoryValue, MatchupTeam, PlayerWeekStats, Position, RosterWeekStats};

fn team(name: &str, remaining_games: i32, stats: &[(&str, &str)]) -> MatchupTeam {
    MatchupTeam {
        team_key: format!("key.{name}"),
        team_id: 1,
        name: name.into(),
        is_current_login: name == "mine",
        stats: stats
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect::<HashMap<_, _>>(),
        wins: 0,
        losses: 0,
        ties: 0,
        completed_games: 0,
        live_games: 0,
        remaining_games,
    }
}

fn player(name: &str, slot: Position, eligible: &[Position], injury: &str) -> PlayerWeekStats {
    PlayerWeekStats {
        yahoo_player_id: 1,
        name: name.into(),
        team: "NYY".into(),
        position_type: "B".into(),
        slot_position: slot,
        eligible_positions: eligible.to_vec(),
        injury_status: injury.into(),
        hab: String::new(),
        runs: 0,
        home_runs: 0,
        runs_batted_in: 0,
        stolen_bases: 0,
        batting_average: String::new(),
        innings_pitched: String::new(),
        wins: 0,
        saves: 0,
        strikeouts: 0,
        earned_run_average: String::new(),
        whip: String::new(),
    }
}

#[test]
fn category_gaps_apply_tie_tolerance_polarity_and_flippability() {
    let mine = team("mine", 0, &[("R", "45"), ("ERA", "3.20"), ("SB", "5")]);
    let opponent = team(
        "them",
        3,
        &[("R", "40"), ("ERA", "3.20"), ("SB", "5.00005")],
    );
    let categories = vec!["R".to_string(), "ERA".to_string(), "SB".to_string()];
    let gaps = compute_category_gaps(&mine, &opponent, &categories, &[]);
    let runs = gaps.iter().find(|gap| gap.category == "R").unwrap();
    assert!(runs.leading && !runs.tied);
    assert!(runs.flippable, "opponent still has remaining games");

    let era = gaps.iter().find(|gap| gap.category == "ERA").unwrap();
    assert!(era.tied && !era.leading);

    let steals = gaps.iter().find(|gap| gap.category == "SB").unwrap();
    assert!(steals.tied, "values within tie tolerance must tie");
}

#[test]
fn category_gaps_mark_lead_unflippable_once_trailing_team_is_out_of_games() {
    let mine = team("mine", 0, &[("HR", "10")]);
    let opponent = team("them", 2, &[("HR", "5")]);
    let categories = vec!["HR".to_string()];
    let gaps = compute_category_gaps(&mine, &opponent, &categories, &[]);
    let hr = &gaps[0];
    assert!(hr.leading);
    assert!(
        hr.flippable,
        "opponent (trailing) still has remaining games"
    );

    let mine_out = team("mine", 0, &[("HR", "10")]);
    let opponent_out = team("them", 0, &[("HR", "5")]);
    let gaps = compute_category_gaps(&mine_out, &opponent_out, &categories, &[]);
    assert!(
        !gaps[0].flippable,
        "trailing team has no remaining games left to close the gap"
    );
}

#[test]
fn category_gaps_mark_punted_categories_from_strategy() {
    let mine = team("mine", 1, &[("SV", "2")]);
    let opponent = team("them", 1, &[("SV", "9")]);
    let gaps = compute_category_gaps(&mine, &opponent, &["SV".to_string()], &["sv".to_string()]);
    assert!(gaps[0].punted);
}

#[test]
fn slot_gaps_report_only_underfilled_positions() {
    let roster = RosterWeekStats {
        team_key: "key".into(),
        team_name: "Mine".into(),
        week: 1,
        players: vec![player("Ada", Position::Outfield, &[Position::Outfield], "")],
    };
    let required = vec![(Position::Outfield, 2), (Position::Catcher, 1)];
    let gaps = compute_slot_gaps(&required, &roster);
    assert_eq!(gaps.len(), 2);
    assert!(gaps.iter().any(|gap| gap.slot == Position::Outfield));
    assert!(gaps.iter().any(|gap| gap.slot == Position::Catcher));
}

#[test]
fn lineup_candidates_require_shared_eligibility_and_exclude_injured_list() {
    let roster = RosterWeekStats {
        team_key: "key".into(),
        team_name: "Mine".into(),
        week: 1,
        players: vec![
            player(
                "Bench Bob",
                Position::Bench,
                &[Position::Outfield, Position::FirstBase],
                "",
            ),
            player("Active Al", Position::Outfield, &[Position::Outfield], ""),
            player(
                "Hurt Hank",
                Position::InjuredList,
                &[Position::Outfield],
                "IL",
            ),
        ],
    };
    let candidates = compute_lineup_candidates(&roster);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].bench_player, "Bench Bob");
    assert_eq!(candidates[0].active_player, "Active Al");
    assert_eq!(candidates[0].position, Position::Outfield);
}

#[test]
fn roster_moves_rank_by_polarity_and_skip_leading_or_punted_gaps() {
    let mine = team("mine", 1, &[("SB", "2"), ("ERA", "5.00")]);
    let opponent = team("them", 1, &[("SB", "9"), ("ERA", "2.00")]);
    let gaps = compute_category_gaps(
        &mine,
        &opponent,
        &["SB".to_string(), "ERA".to_string()],
        &[],
    );
    let candidates = vec![
        FreeAgentCategoryValue {
            player_name: "Speedy".into(),
            category: "SB".into(),
            value: 20.0,
        },
        FreeAgentCategoryValue {
            player_name: "Slow".into(),
            category: "SB".into(),
            value: 5.0,
        },
        FreeAgentCategoryValue {
            player_name: "Ace".into(),
            category: "ERA".into(),
            value: 1.50,
        },
    ];
    let moves = compute_roster_moves(&gaps, &candidates);
    assert_eq!(moves.len(), 2);
    assert!(
        moves
            .iter()
            .any(|candidate| candidate.player_name == "Speedy" && candidate.category == "SB")
    );
    assert!(
        moves
            .iter()
            .any(|candidate| candidate.player_name == "Ace" && candidate.category == "ERA")
    );
}

#[test]
fn risk_alerts_surface_active_injured_players_only() {
    let roster = RosterWeekStats {
        team_key: "key".into(),
        team_name: "Mine".into(),
        week: 1,
        players: vec![
            player("Active Hurt", Position::Outfield, &[], "Day-to-Day"),
            player("Bench Hurt", Position::Bench, &[], "Day-to-Day"),
            player("Healthy", Position::Outfield, &[], ""),
        ],
    };
    let alerts = compute_risk_alerts(&roster);
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].player_name, "Active Hurt");
}

#[test]
fn advisory_context_summarizes_candidates_with_stable_ids() {
    let context = build_advisory_context(
        &[b9::domain::LineupCandidate {
            bench_player: "Bench Bob".into(),
            active_player: "Active Al".into(),
            position: Position::Outfield,
        }],
        &[b9::domain::RosterMoveCandidate {
            player_name: "Speedy".into(),
            category: "SB".into(),
        }],
    );
    assert_eq!(context.lineup_candidates[0].id, "lineup-0");
    assert_eq!(context.roster_moves[0].id, "move-0");
}

#[test]
fn advisory_discards_ungrounded_actions_and_retains_valid_fields() {
    let context = AdvisoryContext {
        lineup_candidates: vec![AdvisoryAction {
            id: "lineup-1".into(),
            summary: "Start Ada".into(),
        }],
        roster_moves: vec![],
    };
    let response = grounded_response(
        &context,
        AdvisoryResponse {
            confirmations: vec!["Rain risk".into()],
            urgent: vec![
                AdvisoryAction {
                    id: "lineup-1".into(),
                    summary: "Start Ada".into(),
                },
                AdvisoryAction {
                    id: "invented".into(),
                    summary: "Invented move".into(),
                },
            ],
            overnight: vec![],
            risks: vec!["Rain".into()],
        },
    );
    assert_eq!(response.urgent.len(), 1);
    assert_eq!(response.confirmations, vec!["Rain risk"]);
    assert_eq!(response.risks, vec!["Rain"]);
}
