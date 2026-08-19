use crate::domain::StoredFantasyPlayer;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerQuality {
    pub score: f64,
    pub rationale: String,
}

#[derive(Clone, Copy)]
struct Signal {
    value: Option<f64>,
    sample: f64,
    threshold: f64,
    weight: f64,
    direction: f64,
    shrink_k: f64,
}

fn signal(
    value: Option<f64>,
    sample: f64,
    threshold: f64,
    weight: f64,
    direction: f64,
    shrink_k: f64,
) -> Signal {
    Signal {
        value: value.filter(|v| *v != 0.0),
        sample,
        threshold,
        weight,
        direction,
        shrink_k,
    }
}

fn signals(p: &StoredFantasyPlayer) -> Vec<Signal> {
    let mut counting = p.pqs_counting;
    let pitcher = p.role == "P";
    let opportunity = if pitcher { p.pitching[0] } else { counting[0] };
    let has_prior = if pitcher {
        p.pqs_prior_counting[3] > 0.0
    } else {
        p.pqs_prior_counting[0] > 0.0
    };
    let weights = crate::analysis::blend::opportunity_dampen(
        crate::analysis::blend::weights(p.league_games_played, has_prior),
        opportunity,
        pitcher,
    );
    if has_prior {
        for (value, prior) in counting.iter_mut().zip(p.pqs_prior_counting) {
            *value = *value * weights.0 + prior * weights.1;
        }
    }
    if p.role == "P" {
        let bf = counting[3];
        vec![
            signal(
                p.pitching_advanced[1],
                p.statcast_samples[2],
                1.0,
                0.30,
                1.0,
                120.0,
            ),
            signal(
                p.pitching_advanced[2],
                p.statcast_samples[2],
                1.0,
                0.20,
                1.0,
                90.0,
            ),
            signal(
                p.pitching_advanced[3],
                p.statcast_samples[3],
                1.0,
                0.15,
                1.0,
                90.0,
            ),
            signal(
                p.pitching_advanced[0],
                p.statcast_samples[2],
                1.0,
                0.15,
                1.0,
                30.0,
            ),
            signal(
                (bf > 0.0).then(|| (counting[4] - counting[5]) / bf),
                bf,
                100.0,
                0.20,
                1.0,
                0.0,
            ),
        ]
    } else {
        let pa = counting[0];
        vec![
            signal(
                p.hitting_advanced[0],
                p.statcast_samples[0],
                1.0,
                0.30,
                1.0,
                165.0,
            ),
            signal(
                (pa > 0.0).then(|| counting[1] / pa),
                pa,
                50.0,
                0.15,
                -1.0,
                0.0,
            ),
            signal(
                (pa > 0.0).then(|| counting[2] / pa),
                pa,
                50.0,
                0.10,
                1.0,
                0.0,
            ),
            signal(p.hitting_advanced[6], 1.0, 1.0, 0.20, 1.0, 0.0),
            signal(p.fangraphs_batted_ball[0], 1.0, 1.0, 0.10, 1.0, 0.0),
            signal(p.fangraphs_batted_ball[1], 1.0, 1.0, 0.15, 1.0, 0.0),
        ]
    }
}

/// Compute pool-normalized PQS values with stabilization and season context.
#[must_use]
pub fn pool_scores(players: &[StoredFantasyPlayer]) -> Vec<PlayerQuality> {
    let extracted = players.iter().map(signals).collect::<Vec<_>>();
    let mut totals = vec![0.0; players.len()];
    for index in 0..extracted.iter().map(Vec::len).max().unwrap_or(0) {
        let values = extracted
            .iter()
            .filter_map(|set| {
                let s = set.get(index)?;
                ((s.sample / s.threshold).min(1.0) >= 0.2)
                    .then_some(s.value?)
                    .or(None)
            })
            .collect::<Vec<_>>();
        if values.is_empty() {
            continue;
        }
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let normalized = extracted
            .iter()
            .filter_map(|set| {
                let signal = set.get(index)?;
                let value = signal.value?;
                Some(crate::analysis::statcast_blend::shrink(
                    value,
                    signal.sample,
                    mean,
                    signal.shrink_k,
                ))
            })
            .collect::<Vec<_>>();
        let normalized_mean = normalized.iter().sum::<f64>() / normalized.len() as f64;
        let sd = (normalized
            .iter()
            .map(|v| (v - normalized_mean).powi(2))
            .sum::<f64>()
            / normalized.len() as f64)
            .sqrt();
        if sd == 0.0 {
            continue;
        }
        for (player_index, set) in extracted.iter().enumerate() {
            let Some(s) = set.get(index) else { continue };
            let Some(raw) = s.value else { continue };
            let value = crate::analysis::statcast_blend::shrink(raw, s.sample, mean, s.shrink_k);
            let ramp = (s.sample / s.threshold).min(1.0);
            if ramp > 0.0 {
                totals[player_index] += (((value - normalized_mean) / sd) * s.direction)
                    .clamp(-2.0, 2.0)
                    * s.weight
                    * ramp;
            }
        }
    }
    for (index, p) in players.iter().enumerate() {
        if p.role == "P" && p.is_closer {
            totals[index] *= 1.5;
        }
    }
    apply_context(players, &mut totals);
    totals
        .into_iter()
        .map(|score| PlayerQuality {
            score,
            rationale: format!("PQS {score:.2}"),
        })
        .collect()
}

fn apply_context(players: &[StoredFantasyPlayer], totals: &mut [f64]) {
    let minimum = totals
        .iter()
        .copied()
        .reduce(f64::min)
        .unwrap_or(0.0)
        .min(0.0);
    let mut by_position = BTreeMap::<String, Vec<f64>>::new();
    for (p, score) in players.iter().zip(totals.iter().copied()) {
        for position in p
            .positions
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            by_position.entry(position.into()).or_default().push(score);
        }
    }
    let replacements = by_position
        .iter_mut()
        .filter_map(|(position, scores)| {
            scores.sort_by(|a, b| b.total_cmp(a));
            scores
                .get(11)
                .or_else(|| scores.last())
                .copied()
                .map(|v| (position.clone(), v))
        })
        .collect::<BTreeMap<_, _>>();
    let mean = if replacements.is_empty() {
        0.0
    } else {
        replacements.values().sum::<f64>() / replacements.len() as f64
    };
    let high = replacements
        .values()
        .copied()
        .reduce(f64::max)
        .unwrap_or(mean);
    let low = replacements
        .values()
        .copied()
        .reduce(f64::min)
        .unwrap_or(mean);
    for (p, score) in players.iter().zip(totals.iter_mut()) {
        let opportunity = if p.percentage_started > 0.0 {
            (p.percentage_started / 80.0).min(1.0)
        } else {
            1.0
        };
        let scarcity = if high == low {
            0.0
        } else {
            p.positions
                .split(',')
                .filter_map(|position| replacements.get(position.trim()))
                .map(|v| ((mean - v) / (high - low) * 0.15).clamp(0.0, 0.15))
                .fold(0.0, f64::max)
        };
        *score = (*score - minimum) * opportunity * (1.0 + scarcity) + minimum;
    }
}

#[must_use]
pub fn score(player: &StoredFantasyPlayer) -> PlayerQuality {
    pool_scores(std::slice::from_ref(player)).remove(0)
}

pub fn sort_by_pqs(players: &mut [StoredFantasyPlayer]) {
    let scores = pool_scores(players);
    let by_id = players
        .iter()
        .zip(scores)
        .map(|(p, s)| ((p.yahoo_player_id, p.name.clone()), s.score))
        .collect::<BTreeMap<_, _>>();
    players.sort_by(|a, b| {
        by_id
            .get(&(b.yahoo_player_id, b.name.clone()))
            .copied()
            .unwrap_or_default()
            .total_cmp(
                &by_id
                    .get(&(a.yahoo_player_id, a.name.clone()))
                    .copied()
                    .unwrap_or_default(),
            )
            .then_with(|| a.name.cmp(&b.name))
    });
}
