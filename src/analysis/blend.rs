/// Scheduled current/prior weights for the league-games window.
#[must_use]
pub fn weights(games: i64, has_prior: bool) -> (f64, f64) {
    if !has_prior {
        return (1.0, 0.0);
    }
    match games {
        0..=7 => (0.15 / 0.95, 0.80 / 0.95),
        8..=14 => (0.25, 0.75),
        15..=27 => (0.50, 0.50),
        _ => (1.0, 0.0),
    }
}

/// Dampen current-season weight until a hitter reaches 150 PA or pitcher 40 IP.
#[must_use]
pub fn opportunity_dampen(weights: (f64, f64), opportunity: f64, pitcher: bool) -> (f64, f64) {
    if weights.1 == 0.0 {
        return weights;
    }
    let ramp = (opportunity / if pitcher { 40.0 } else { 150.0 }).clamp(0.0, 1.0);
    let current = weights.0 * ramp;
    (current, weights.1 + weights.0 - current)
}

#[must_use]
pub fn value(current: f64, prior: Option<f64>, games: i64) -> f64 {
    let (cw, pw) = weights(games, prior.is_some());
    current * cw + prior.unwrap_or(0.0) * pw
}
