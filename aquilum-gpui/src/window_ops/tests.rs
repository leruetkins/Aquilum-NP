use super::*;

#[test]
fn net_wm_state_event_adds_above() {
    let event = net_wm_state_event(7, 31, 42, 0, true);

    assert_eq!(event.format, 32);
    assert_eq!(event.window, 7);
    assert_eq!(event.type_, 31);
    assert_eq!(event.data.as_data32(), [1, 42, 0, 1, 0]);
}

#[test]
fn net_wm_state_event_removes_above() {
    let event = net_wm_state_event(7, 31, 42, 0, false);

    assert_eq!(event.data.as_data32(), [0, 42, 0, 1, 0]);
}

#[test]
fn net_wm_state_event_maximizes_both_axes() {
    let event = net_wm_state_event(7, 31, 40, 41, true);

    assert_eq!(event.data.as_data32(), [1, 40, 41, 1, 0]);
}
