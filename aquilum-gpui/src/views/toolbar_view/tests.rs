use super::strip::toolbar_visible_count;

/// Widths of the bundled toolbar: 32px buttons, 44px dropdowns, 11px separators.
fn default_widths() -> Vec<f32> {
    vec![
        32.0, 32.0, 32.0, 11.0, 32.0, 32.0, 32.0, 44.0, 11.0, 32.0, 32.0, 32.0, 32.0, 32.0,
        11.0, 32.0, 32.0, 32.0, 32.0, 32.0, 11.0, 44.0, 44.0, 44.0, 44.0, 11.0, 32.0,
    ]
}

/// Width of the row: visible icons, their gaps and the overflow button.
fn content_width(widths: &[f32], visible: usize, overflow_btn_w: f32) -> f32 {
    let mut width = widths[..visible].iter().sum::<f32>()
        + 3.0 * visible.saturating_sub(1) as f32;
    if visible < widths.len() {
        width += if visible == 0 { overflow_btn_w } else { 3.0 + overflow_btn_w };
    }
    width
}

#[test]
fn a_centered_row_always_fits_its_strip() {
    let widths = default_widths();
    for available in 100..=1600 {
        let max_w = (available as f32 - 24.0).max(0.0);
        let visible = toolbar_visible_count(&widths, max_w, 44.0);
        // 8px padding and a 3px gap on each side of a centered row.
        let row = content_width(&widths, visible, 44.0) + 22.0;
        assert!(
            row <= available as f32,
            "row overflows: available={available}, visible={visible}, row={row}"
        );
    }
}

#[test]
fn widening_the_strip_never_hides_more_items() {
    let widths = default_widths();
    let mut previous = 0;
    for available in 66..=1600 {
        let max_w = (available as f32 - 24.0).max(0.0);
        let visible = toolbar_visible_count(&widths, max_w, 44.0);
        assert!(
            visible >= previous,
            "visible icons dropped at {available}px: {previous} -> {visible}"
        );
        previous = visible;
    }
}

#[test]
fn items_after_the_cut_never_return_to_the_strip() {
    let widths = default_widths();
    // At these widths the old per-item check kept the last icon (fullscreen) on the
    // strip even though everything around it was already in the overflow menu.
    for available in [400.0_f32, 600.0, 719.0, 800.0, 900.0] {
        let max_w = available - 24.0;
        let visible = toolbar_visible_count(&widths, max_w, 44.0);
        assert!(visible < widths.len(), "no overflow at {available}px");
        assert!(
            content_width(&widths, visible, 44.0) + 22.0 <= available,
            "row overflows at {available}px: visible={visible}"
        );
        // The tail, including the last icon, is behind the overflow button.
        assert!(
            visible + 1 < widths.len(),
            "last icon leaked onto the strip at {available}px"
        );
    }
}
