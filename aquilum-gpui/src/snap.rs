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
}
