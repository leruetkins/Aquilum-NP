use crate::state::SessionState;

#[test]
fn old_session_without_always_on_top_defaults_to_false() {
    // Sessions written before the flag existed must not pin the window.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("session-state.json"),
        r#"{"sidebar_width":260.0}"#,
    )
    .unwrap();

    assert!(!SessionState::load(dir.path()).always_on_top);
}

#[test]
fn always_on_top_round_trips_through_session_file() {
    let dir = tempfile::tempdir().unwrap();
    let state = SessionState {
        always_on_top: true,
        ..SessionState::default()
    };
    state.save(dir.path());

    assert!(SessionState::load(dir.path()).always_on_top);
}
