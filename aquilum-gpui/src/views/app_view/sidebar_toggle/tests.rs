use super::*;

fn at(x: f32, y: f32) -> gpui::Point<Pixels> {
    gpui::Point::new(px(x), px(y))
}

#[test]
fn drag_starts_only_at_the_threshold() {
    assert!(!is_window_drag(at(10.0, 10.0), at(12.9, 10.0)));
    assert!(is_window_drag(at(10.0, 10.0), at(13.0, 10.0)));
}

#[test]
fn drag_ignores_direction() {
    assert!(is_window_drag(at(10.0, 10.0), at(10.0, 20.0)));
    assert!(is_window_drag(at(10.0, 10.0), at(0.0, 10.0)));
}
