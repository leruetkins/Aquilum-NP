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
}
