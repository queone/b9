//! Transparent deterministic season-stat evaluation for roster and waiver views.

use crate::domain::StoredFantasyPlayer;

/// One player score with its displayed ranking rationale.
#[derive(Clone, Debug, PartialEq)]
pub struct Evaluation {
    pub score: f64,
    pub rationale: String,
}

/// Score a player from durable season counting statistics and rates.
#[must_use]
pub fn evaluate(player: &StoredFantasyPlayer) -> Evaluation {
    let (score, rationale) = if player.role == "B" {
        let score = player.batting[2]
            + player.batting[3] * 4.0
            + player.batting[4] * 1.5
            + player.batting[5] * 3.0
            + player.batting[6] * 100.0;
        (
            score,
            format!(
                "R {:.0} HR {:.0} RBI {:.0} SB {:.0} AVG {:.3}",
                player.batting[2],
                player.batting[3],
                player.batting[4],
                player.batting[5],
                player.batting[6]
            ),
        )
    } else {
        let rate_score = if player.pitching[0] > 0.0 {
            (5.0 - player.pitching[5]) * 5.0 + (2.0 - player.pitching[6]) * 10.0
        } else {
            0.0
        };
        let score =
            player.pitching[2] * 6.0 + player.pitching[3] * 7.0 + player.pitching[4] + rate_score;
        (
            score,
            format!(
                "W {:.0} SV {:.0} K {:.0} ERA {:.2} WHIP {:.2}",
                player.pitching[2],
                player.pitching[3],
                player.pitching[4],
                player.pitching[5],
                player.pitching[6]
            ),
        )
    };
    Evaluation { score, rationale }
}

/// Sort players by evaluation with deterministic name tie-breaking.
pub fn sort_by_evaluation(players: &mut [StoredFantasyPlayer]) {
    players.sort_by(|left, right| {
        evaluate(right)
            .score
            .total_cmp(&evaluate(left).score)
            .then_with(|| left.name.cmp(&right.name))
    });
}
