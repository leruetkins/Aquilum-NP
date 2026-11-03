//! Device-pixel snapping for thin overlay geometry (text carets).
//!
//! GPUI paints quads at raw float coordinates with no device-pixel snapping
//! (`paint_quad` only scales logical bounds by the scale factor), so a
//! 1.5-2px caret at a fractional X covers physical pixels with varying
//! splits and visibly changes thickness while typing — a different letter
//! lands the caret on a different pixel phase. Snapping the caret X to the
//! physical grid keeps its look stable without changing its design width.

/// Snaps a logical-pixel `x` to the physical pixel grid for `scale_factor`.
///
/// Degenerate factors fall back to the input unchanged.
pub fn snap_x(x: f32, scale_factor: f32) -> f32 {
    if !x.is_finite() || !scale_factor.is_finite() || scale_factor <= 0.0 {
        return x;
    }
    (x * scale_factor).round() / scale_factor
}

/// Snaps both edges of a span to the physical pixel grid and returns
/// `(left, width)`.
///
/// Selection rectangles use this so their edges land on the same device pixels as
/// the caret. GPUI paints a quad with rounded corners on its signed-distance path
/// with an antialiasing threshold, so an unsnapped selection edge is blended across
/// the pixel it straddles: the blend ratio shifts with every sub-pixel mouse move
/// and the edge visibly breathes, while the snapped caret on the same boundary stays
/// crisp.
///
/// Rounding cannot reorder the edges, so a reversed or empty span yields a zero
/// width rather than a negative one.
pub fn snapped_span(x1: f32, x2: f32, scale_factor: f32) -> (f32, f32) {
    let left = snap_x(x1, scale_factor);
    let right = snap_x(x2, scale_factor);
    (left, (right - left).max(0.0))
}

/// Snaps the left edge of a thin vertical rule to the physical grid and returns
/// `(left, width)` with a constant width of `device_width` physical pixels.
///
/// [`snapped_span`] rounds both edges, so a 2-logical-pixel rule lands on two or
/// three device pixels depending on which column it starts in: at 125% a bar in
/// one quote level is thinner than the bar of the next. A rule that is part of a
/// group — quote bars, list indent guides — must keep one thickness, so only its
/// position is snapped and the width is taken from the device grid directly.
pub fn snapped_rule(x: f32, device_width: f32, scale_factor: f32) -> (f32, f32) {
    let left = snap_x(x, scale_factor);
    (left, device_thickness(device_width, scale_factor))
}

/// Logical thickness of a rule that has to cover `device_width` physical pixels.
///
/// A rule drawn with a logical size lands on a whole number of device pixels only
/// at 100% and 200%. At 125% or 150% a 1-logical-pixel line is 1.25 or 1.5 device
/// pixels, so the quad is blended across two pixels at partial alpha and fades
/// into the background. Taking the thickness from the device grid keeps it solid.
/// Degenerate factors yield zero, like [`snapped_rule`].
pub fn device_thickness(device_width: f32, scale_factor: f32) -> f32 {
    if !device_width.is_finite() || !scale_factor.is_finite() || scale_factor <= 0.0 {
        return 0.0;
    }
    device_width.max(1.0) / scale_factor
}

#[cfg(test)]
mod tests;
