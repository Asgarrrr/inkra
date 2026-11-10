//! A split's flexes: one per child, each above 0, summing to their count.

/// Zed's flex epsilon (S1).
pub(crate) const EPSILON: f32 = 0.001;

/// As Zed's `PaneAxis::load`: one per child, each finite and above 0, summing to their count.
pub(crate) fn valid(flexes: &[f32], len: usize) -> bool {
    flexes.len() == len
        && flexes.iter().all(|&flex| flex.is_finite() && flex > 0.0)
        && (flexes.iter().sum::<f32>() - count(len)).abs() <= EPSILON
}

/// Keeps each flex's share.
pub(crate) fn rescale(flexes: &mut [f32]) {
    let scale = count(flexes.len()) / flexes.iter().sum::<f32>();
    for flex in flexes {
        *flex *= scale;
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a count far below 2^24 converts to f32 exactly"
)]
pub(crate) fn count(len: usize) -> f32 {
    len as f32
}
