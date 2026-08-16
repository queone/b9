//! Deterministic matchup analysis and provider-neutral, grounded advisory contracts.

use serde::{Deserialize, Serialize};

use crate::domain::{
    CategoryGap, FreeAgentCategoryValue, LOWER_IS_BETTER_CATEGORIES, LineupCandidate, MatchupTeam,
    Position, RiskAlert, RosterMoveCandidate, RosterWeekStats, SlotGap,
};
use crate::strategy::is_punted;

const TIE_TOLERANCE: f64 = 0.0001;

/// Compute ordered category gaps between the two matchup teams.
///
/// Flippability approximates skout's remaining-games heuristic: a lead is
/// treated as safe only once the trailing team has no remaining games left
/// to close it, since b9 does not yet replicate skout's exact magnitude-scaled
/// lead/lost thresholds per category (tracked for later tuning against live
/// output, not a hard behavioral contract).
#[must_use]
pub fn compute_category_gaps(
    mine: &MatchupTeam,
    opponent: &MatchupTeam,
    categories: &[String],
    punts: &[String],
) -> Vec<CategoryGap> {
    categories
        .iter()
        .filter_map(|category| {
            let mine_value = parse_category_value(mine, category)?;
            let opponent_value = parse_category_value(opponent, category)?;
            let lower_is_better = LOWER_IS_BETTER_CATEGORIES.contains(&category.as_str());
            let tied = (mine_value - opponent_value).abs() < TIE_TOLERANCE;
            let leading = !tied
                && if lower_is_better {
                    mine_value < opponent_value
                } else {
                    mine_value > opponent_value
                };
            let trailing_team = if leading { opponent } else { mine };
            let flippable = !tied && trailing_team.remaining_games > 0;
            Some(CategoryGap {
                category: category.clone(),
                mine: mine_value,
                opponent: opponent_value,
                tied,
                leading,
                flippable,
                punted: is_punted(category, punts),
            })
        })
        .collect()
}

fn parse_category_value(team: &MatchupTeam, category: &str) -> Option<f64> {
    team.stats.get(category)?.trim().parse().ok()
}

/// Compute roster slots the league requires but the current roster leaves uncovered.
#[must_use]
pub fn compute_slot_gaps(required: &[(Position, i64)], roster: &RosterWeekStats) -> Vec<SlotGap> {
    required
        .iter()
        .filter_map(|(position, count)| {
            let filled = roster
                .players
                .iter()
                .filter(|player| player.slot_position == *position)
                .count() as i64;
            (filled < *count).then_some(SlotGap {
                slot: position.clone(),
            })
        })
        .collect()
}

/// Compute deterministic bench-for-active lineup swaps grounded in shared position eligibility.
#[must_use]
pub fn compute_lineup_candidates(roster: &RosterWeekStats) -> Vec<LineupCandidate> {
    let bench = roster
        .players
        .iter()
        .filter(|player| player.slot_position == Position::Bench);
    let active = roster
        .players
        .iter()
        .filter(|player| {
            !matches!(
                player.slot_position,
                Position::Bench | Position::InjuredList
            )
        })
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();
    for benched in bench {
        for starter in &active {
            if benched.eligible_positions.contains(&starter.slot_position) {
                candidates.push(LineupCandidate {
                    bench_player: benched.name.clone(),
                    active_player: starter.name.clone(),
                    position: starter.slot_position.clone(),
                });
            }
        }
    }
    candidates
}

/// Compute free-agent roster-move candidates grounded in flippable, non-punted category gaps.
#[must_use]
pub fn compute_roster_moves(
    gaps: &[CategoryGap],
    candidates: &[FreeAgentCategoryValue],
) -> Vec<RosterMoveCandidate> {
    let mut moves = Vec::new();
    for gap in gaps
        .iter()
        .filter(|gap| gap.flippable && !gap.leading && !gap.punted)
    {
        let lower_is_better = LOWER_IS_BETTER_CATEGORIES.contains(&gap.category.as_str());
        let mut ranked = candidates
            .iter()
            .filter(|candidate| candidate.category == gap.category)
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            if lower_is_better {
                left.value.total_cmp(&right.value)
            } else {
                right.value.total_cmp(&left.value)
            }
        });
        if let Some(top) = ranked.first() {
            moves.push(RosterMoveCandidate {
                player_name: top.player_name.clone(),
                category: gap.category.clone(),
            });
        }
    }
    moves
}

/// Compute risk alerts from roster injury and availability state.
#[must_use]
pub fn compute_risk_alerts(roster: &RosterWeekStats) -> Vec<RiskAlert> {
    roster
        .players
        .iter()
        .filter(|player| {
            !player.injury_status.trim().is_empty() && player.slot_position != Position::Bench
        })
        .map(|player| RiskAlert {
            player_name: player.name.clone(),
            reason: player.injury_status.clone(),
        })
        .collect()
}

/// One action computed from a deterministic matchup candidate pool.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdvisoryAction {
    pub id: String,
    pub summary: String,
}

/// The only actions an advisory response may recommend.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdvisoryContext {
    pub lineup_candidates: Vec<AdvisoryAction>,
    pub roster_moves: Vec<AdvisoryAction>,
}

/// One partially valid provider response after grounding.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdvisoryResponse {
    pub confirmations: Vec<String>,
    pub urgent: Vec<AdvisoryAction>,
    pub overnight: Vec<AdvisoryAction>,
    pub risks: Vec<String>,
}

/// Build the grounded action pool an advisory response may draw from.
#[must_use]
pub fn build_advisory_context(
    lineup_candidates: &[LineupCandidate],
    roster_moves: &[RosterMoveCandidate],
) -> AdvisoryContext {
    AdvisoryContext {
        lineup_candidates: lineup_candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| AdvisoryAction {
                id: format!("lineup-{index}"),
                summary: format!(
                    "Start {} over {} at {}",
                    candidate.bench_player, candidate.active_player, candidate.position
                ),
            })
            .collect(),
        roster_moves: roster_moves
            .iter()
            .enumerate()
            .map(|(index, candidate)| AdvisoryAction {
                id: format!("move-{index}"),
                summary: format!(
                    "Add {} to help {}",
                    candidate.player_name, candidate.category
                ),
            })
            .collect(),
    }
}

/// Discard advisory actions that are absent from the supplied deterministic context.
pub fn grounded_response(
    context: &AdvisoryContext,
    mut response: AdvisoryResponse,
) -> AdvisoryResponse {
    let allowed = context
        .lineup_candidates
        .iter()
        .chain(&context.roster_moves)
        .map(|action| action.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    response
        .urgent
        .retain(|action| allowed.contains(action.id.as_str()));
    response
        .overnight
        .retain(|action| allowed.contains(action.id.as_str()));
    response
}
