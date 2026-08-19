#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PitcherRole {
    Starter,
    Reliever,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecentAppearance {
    pub games_started: i64,
}

/// Classify from probable status, recent use, season use, then eligibility.
#[must_use]
pub fn classify_with_context(
    positions: &str,
    probable: bool,
    recent: &[RecentAppearance],
    season_games: f64,
    season_starts: f64,
) -> PitcherRole {
    if probable
        || recent
            .iter()
            .take(5)
            .filter(|game| game.games_started > 0)
            .count()
            >= 3
    {
        return PitcherRole::Starter;
    }
    if season_games > 0.0 {
        let ratio = season_starts / season_games;
        if ratio >= 0.6 {
            return PitcherRole::Starter;
        }
        if ratio < 0.35 {
            return PitcherRole::Reliever;
        }
    }
    classify(positions)
}
#[must_use]
pub fn classify(positions: &str) -> PitcherRole {
    if positions.split(',').any(|p| p.trim() == "SP") {
        PitcherRole::Starter
    } else {
        PitcherRole::Reliever
    }
}
