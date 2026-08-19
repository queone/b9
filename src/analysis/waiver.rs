use crate::domain::StoredFantasyPlayer;
#[must_use]
pub fn eligible(player: &StoredFantasyPlayer, floor: f64) -> bool {
    player.mlbam_id.is_some()
        && !matches!(player.status.as_str(), "NA" | "SUSP")
        && !player.status.starts_with("IL")
        && if player.role == "P" {
            player.pitching[0] >= floor
        } else {
            player.batting[0] >= floor
        }
}
