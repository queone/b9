/// Empirical-Bayes shrinkage toward a pool mean.
#[must_use]
pub fn shrink(raw: f64, sample: f64, mean: f64, k: f64) -> f64 {
    if sample <= 0.0 {
        mean
    } else {
        (raw * sample + mean * k) / (sample + k)
    }
}
