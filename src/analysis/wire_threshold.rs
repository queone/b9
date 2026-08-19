/// R-7 linearly interpolated percentile.
#[must_use]
pub fn percentile(mut values: Vec<f64>, p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    if values.len() == 1 {
        return Some(values[0]);
    }
    let h = (values.len() - 1) as f64 * p.clamp(0.0, 1.0);
    let lo = h.floor() as usize;
    let hi = h.ceil() as usize;
    Some(values[lo] + (values[hi] - values[lo]) * (h - lo as f64))
}
