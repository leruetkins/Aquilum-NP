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
    // Same for the right-sidebar selection written by newer builds.
    let loaded = SessionState::load(dir.path());
    assert!(!loaded.right_sidebar_outline_open);
    assert!(loaded.right_sidebar_leaf.is_none());
    assert_eq!(loaded.right_sidebar_width, 0.0);
    assert!(loaded.right_sidebar_collapsed);
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

#[test]
fn right_sidebar_selection_round_trips_through_session_file() {
    let dir = tempfile::tempdir().unwrap();
    let state = SessionState {
        right_sidebar_outline_open: true,
        right_sidebar_leaf: Some("backlinks".to_string()),
        right_sidebar_width: 320.0,
        right_sidebar_collapsed: false,
        ..SessionState::default()
    };
    state.save(dir.path());

    let loaded = SessionState::load(dir.path());
    assert!(loaded.right_sidebar_outline_open);
    assert_eq!(loaded.right_sidebar_leaf.as_deref(), Some("backlinks"));
    assert_eq!(loaded.right_sidebar_width, 320.0);
    assert!(!loaded.right_sidebar_collapsed);
}
