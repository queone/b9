//! Deterministic matchup strategy settings.

/// Return whether a scoring category is intentionally punted.
pub fn is_punted(category: &str, punts: &[String]) -> bool {
    punts
        .iter()
        .any(|punt| punt.trim().eq_ignore_ascii_case(category.trim()))
}

/// Normalize strategy category names for durable comparison.
pub fn normalized_punts(punts: &[String]) -> Vec<String> {
    let mut values = punts
        .iter()
        .map(|punt| punt.trim().to_ascii_lowercase())
        .filter(|punt| !punt.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}
