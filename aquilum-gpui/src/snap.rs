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
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.0001
    }

    #[test]
    fn integer_scale_snaps_to_whole_pixels() {
        assert!(approx(snap_x(10.4, 1.0), 10.0));
        assert!(approx(snap_x(10.5, 1.0), 11.0));
        assert!(approx(snap_x(10.5, 2.0), 10.5));
        assert!(approx(snap_x(10.24, 2.0), 10.0));
    }

    #[test]
    fn fractional_scale_snaps_to_device_grid() {
        // 150%: device pixels sit on 2/3-logical boundaries.
        assert!(approx(snap_x(10.0, 1.5), 10.0));
        assert!(approx(snap_x(10.4, 1.5), 16.0 / 1.5));
        // Snapped values always land on the device grid.
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for i in 0..100 {
                let snapped = snap_x(i as f32 * 0.37, scale);
                assert!(
                    approx(snapped * scale, (snapped * scale).round()),
                    "x={} scale={}",
                    snapped,
                    scale
                );
            }
        }
    }

    #[test]
    fn degenerate_inputs_pass_through() {
        assert_eq!(snap_x(10.3, 0.0), 10.3);
        assert!(snap_x(f32::NAN, 1.0).is_nan());
    }

    /// A selection rectangle's edges must sit on the device grid exactly like the
    /// caret's, or the antialiased edge blends differently at every sub-pixel mouse
    /// position and the selection appears to breathe while dragging.
    #[test]
    fn snapped_span_puts_both_edges_on_the_device_grid() {
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
            for (x1, x2) in [(10.4, 42.7), (0.0, 0.0), (7.13, 7.99), (120.5, 121.5)] {
                let (left, width) = snapped_span(x1, x2, scale);
                assert!(width >= 0.0, "negative width for {x1}..{x2} @{scale}x");
                for edge in [left, left + width] {
                    let device = edge * scale;
                    assert!(
                        (device - device.round()).abs() < 0.0001,
                        "edge {edge} of {x1}..{x2} @{scale}x is off the device grid ({device})"
                    );
                }
            }
        }
    }

    /// Snapping must only round: a reversed span collapses to zero width instead of
    /// flipping sign, and no edge may wander further than half a device pixel.
    #[test]
    fn snapped_span_only_rounds() {
        let (_, width) = snapped_span(20.0, 10.0, 1.5);
        assert_eq!(width, 0.0);
        let (_, empty) = snapped_span(42.3, 42.3, 1.5);
        assert_eq!(empty, 0.0);

        for scale in [1.0, 1.5, 1.75] {
            for i in 0..200 {
                let x1 = i as f32 * 0.37;
                let x2 = x1 + 5.3;
                let (left, width) = snapped_span(x1, x2, scale);
                assert!((left - x1).abs() <= 0.5 / scale + 0.0001, "left edge moved too far");
                assert!(
                    ((left + width) - x2).abs() <= 0.5 / scale + 0.0001,
                    "right edge moved too far"
                );
            }
        }
    }

    /// A group of rules must have one thickness: the same `device_width` in every
    /// column and at every scale, with only the position snapping. This is what
    /// `snapped_span` cannot promise — there a 2-logical-pixel bar covers two or
    /// three device pixels depending on where the column starts, so a deeply
    /// nested quote would draw levels of visibly different weight.
    #[test]
    fn snapped_rule_keeps_one_thickness_in_every_column() {
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
            for device_width in [1.0, 2.0, 3.0] {
                for col in 0..40 {
                    let (left, width) = snapped_rule(col as f32 * 7.3, device_width, scale);
                    let device_left = left * scale;
                    assert!(
                        (device_left - device_left.round()).abs() < 0.0001,
                        "left edge off the grid at col={col} scale={scale}"
                    );
                    assert!(
                        (width * scale - device_width).abs() < 0.0001,
                        "width {} device px instead of {device_width} at col={col} scale={scale}",
                        width * scale
                    );
                }
            }
        }
    }

    #[test]
    fn snapped_rule_survives_degenerate_inputs() {
        let (_, width) = snapped_rule(10.0, 2.0, 0.0);
        assert_eq!(width, 0.0);
        // Sub-pixel rules still cover a whole device pixel or they vanish.
        let (_, hairline) = snapped_rule(3.0, 0.2, 2.0);
        assert!((hairline * 2.0 - 1.0).abs() < 0.0001);
    }

    /// A horizontal rule has no column to snap to, so its thickness is the only
    /// thing that keeps it solid: whatever the scale, the drawn line has to cover
    /// a whole number of device pixels instead of a fractional one blended into
    /// the background.
    #[test]
    fn device_thickness_covers_whole_device_pixels() {
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
            for device_width in [1.0, 2.0, 3.0] {
                let logical = device_thickness(device_width, scale);
                assert!(
                    (logical * scale - device_width).abs() < 0.0001,
                    "{device_width} device px at {scale}x came out as {}",
                    logical * scale
                );
            }
        }
        assert_eq!(device_thickness(2.0, 0.0), 0.0);
        // Never thinner than one device pixel.
        assert!((device_thickness(0.2, 2.0) * 2.0 - 1.0).abs() < 0.0001);
    }
}
