use crate::*;

fn ins(at: usize, t: &str) -> EditCmd {
    EditCmd::Insert { at, text: t.into() }
}

/// Two leaves, one note open in both. Returns (ws, tab A, tab B, doc).
fn setup(content: &str) -> (Workspace, TabId, TabId, DocumentId) {
    let mut ws = Workspace::new();
    let l0 = ws.layout.active_leaf;
    let l1 = ws.split_leaf(l0, Axis::Horizontal).unwrap();
    let a = ws.open_in_leaf(l0, "/v/a.md", content).unwrap();
    let b = ws.open_in_leaf(l1, "/v/a.md", content).unwrap();
    let doc = ws.tabs.get(a).unwrap().doc_id;
    (ws, a, b, doc)
}

fn set_cursor(ws: &mut Workspace, t: TabId, off: usize, anchor: Option<usize>) {
    let s = ws.tabs.get_mut(t).unwrap();
    s.cursor.offset = off;
    s.cursor.anchor = anchor;
}

/// Closing by protocol: effects, then finish_close.
fn close_clean(ws: &mut Workspace, t: TabId) {
    if let CloseRequest::NeedsEffects(fx) = ws.request_close(t).unwrap() {
        for e in &fx {
            ws.run_effect(e).unwrap();
        }
    }
    ws.finish_close(t).unwrap();
}

// 1 ──────────────────────────────────────────────────────────────────
#[test]
fn edit_above_shifts_cursor_selection_and_viewport_in_other_tab() {
    let (mut ws, a, b, _) = setup("line1\nline2\nline3");
    set_cursor(&mut ws, b, 12, Some(6));
    ws.tabs.get_mut(b).unwrap().view.anchor_offset = 6;

    ws.edit(a, ins(0, "ab")).unwrap();

    let sb = ws.tabs.get(b).unwrap();
    assert_eq!(sb.cursor.offset, 14);
    assert_eq!(sb.cursor.anchor, Some(8));
    assert_eq!(sb.view.anchor_offset, 8);
}

#[test]
fn edit_below_does_not_move_other_tab() {
    let (mut ws, a, b, _) = setup("line1\nline2\nline3");
    set_cursor(&mut ws, b, 3, None);
    ws.edit(a, ins(15, "zz")).unwrap();
    assert_eq!(ws.tabs.get(b).unwrap().cursor.offset, 3);
}

// 2 ──────────────────────────────────────────────────────────────────
#[test]
fn origin_gets_cursor_after_and_is_not_transformed_twice() {
    let (mut ws, a, _b, _) = setup("0123456789ABCDEF");
    set_cursor(&mut ws, a, 10, None);
    // edit not at the cursor position (e.g. a search replace)
    ws.edit(a, ins(3, "abc")).unwrap();
    // cursor_after = 3 + 3; transform would give 13
    assert_eq!(ws.tabs.get(a).unwrap().cursor.offset, 6);
}

// 3 ──────────────────────────────────────────────────────────────────
#[test]
fn undo_from_other_tab_restores_text_and_cursors() {
    let (mut ws, a, b, doc) = setup("hello world");
    set_cursor(&mut ws, b, 11, None);
    ws.edit(a, ins(0, "XX")).unwrap();
    assert_eq!(ws.docs.get(doc).unwrap().content(), "XXhello world");
    assert_eq!(ws.tabs.get(b).unwrap().cursor.offset, 13);

    let ev = ws.undo(b).unwrap().expect("есть что откатывать");
    assert_eq!(ev.origin, Some(b));
    assert_eq!(ws.docs.get(doc).unwrap().content(), "hello world");
    assert_eq!(ws.tabs.get(b).unwrap().cursor.offset, 0); // cursor_after for the undo initiator
    assert_eq!(ws.tabs.get(a).unwrap().cursor.offset, 0); // 2 → 0 via Delete{0,2}

    ws.redo(b).unwrap().unwrap();
    assert_eq!(ws.docs.get(doc).unwrap().content(), "XXhello world");
    assert_eq!(ws.tabs.get(b).unwrap().cursor.offset, 2);
}

#[test]
fn undo_of_delete_reinserts_text() {
    let (mut ws, a, _b, doc) = setup("abcdef");
    ws.edit(a, EditCmd::Delete { at: 1, len: 3 }).unwrap();
    assert_eq!(ws.docs.get(doc).unwrap().content(), "aef");
    ws.undo(a).unwrap().unwrap();
    assert_eq!(ws.docs.get(doc).unwrap().content(), "abcdef");
}

// 4 ──────────────────────────────────────────────────────────────────
#[test]
fn close_with_title_input_requires_commit_and_session_survives_until_finish() {
    let (mut ws, a, b, doc) = setup("x");
    ws.tabs.get_mut(a).unwrap().title_input =
        Some(TitleInput { text: "New".into(), cursor: 3, anchor: None });

    let req = ws.request_close(a).unwrap();
    assert_eq!(
        req,
        CloseRequest::NeedsEffects(vec![Effect::CommitTitle { tab: a, doc, new_title: "New".into() }])
    );
    assert!(ws.tabs.get(a).is_some(), "request_close ничего не удаляет");
    assert_eq!(ws.finish_close(a), Err(WsError::PendingEffects));

    if let CloseRequest::NeedsEffects(fx) = req {
        for e in &fx {
            ws.run_effect(e).unwrap();
        }
    }
    ws.finish_close(a).unwrap();
    // doc is alive (tab B exists), the title is applied and visible in B without broadcasting
    assert_eq!(ws.title_of(b), Some("New"));
}

#[test]
fn failed_title_commit_cancels_close() {
    let mut ws = Workspace::new();
    let l = ws.layout.active_leaf;
    let a = ws.open_in_leaf(l, "/v/a.md", "").unwrap();
    let _b = ws.open_in_leaf(l, "/v/b.md", "").unwrap();
    ws.tabs.get_mut(a).unwrap().title_input =
        Some(TitleInput { text: "B".into(), cursor: 1, anchor: None }); // collision (case is normalized)

    let CloseRequest::NeedsEffects(fx) = ws.request_close(a).unwrap() else { panic!() };
    assert_eq!(ws.run_effect(&fx[0]), Err(WsError::NameTaken));
    assert_eq!(ws.finish_close(a), Err(WsError::PendingEffects));
    assert!(ws.tabs.get(a).unwrap().title_input.is_some(), "ввод не потерян");
    assert_eq!(ws.title_of(a), Some("a"));
}

// 5 ──────────────────────────────────────────────────────────────────
#[test]
fn closing_neighbours_does_not_leak_state_into_remaining_tab() {
    let mut ws = Workspace::new();
    let l = ws.layout.active_leaf;
    let a = ws.open_in_leaf(l, "/v/a.md", "aaaa").unwrap();
    let b = ws.open_in_leaf(l, "/v/b.md", "bbbbbbbb").unwrap();
    let c = ws.open_in_leaf(l, "/v/c.md", "cc").unwrap();
    {
        let s = ws.tabs.get_mut(b).unwrap();
        s.cursor.offset = 3;
        s.view.scroll_top = 120.0;
        s.title_input = Some(TitleInput { text: "draft".into(), cursor: 5, anchor: None });
    }
    ws.tabs.get_mut(a).unwrap().view.scroll_top = 5.0;
    ws.tabs.get_mut(c).unwrap().view.scroll_top = 9.0;

    close_clean(&mut ws, a);
    close_clean(&mut ws, c);

    let sb = ws.tabs.get(b).unwrap();
    assert_eq!(sb.cursor.offset, 3);
    assert_eq!(sb.view.scroll_top, 120.0);
    assert_eq!(sb.title_input.as_ref().unwrap().text, "draft");
    assert_eq!(ws.title_of(b), Some("b"));
    assert_eq!(ws.layout.leaf(l).unwrap().tabs, vec![b]);
    assert_eq!(ws.layout.leaf(l).unwrap().active, Some(b));
    assert_eq!(ws.docs.len(), 1);
}

#[test]
fn closing_active_tab_activates_neighbour() {
    let mut ws = Workspace::new();
    let l = ws.layout.active_leaf;
    let a = ws.open_in_leaf(l, "/v/a.md", "").unwrap();
    let b = ws.open_in_leaf(l, "/v/b.md", "").unwrap();
    let c = ws.open_in_leaf(l, "/v/c.md", "").unwrap();
    ws.select_tab(l, b).unwrap();
    close_clean(&mut ws, b);
    assert_eq!(ws.layout.leaf(l).unwrap().active, Some(c));
    close_clean(&mut ws, c);
    assert_eq!(ws.layout.leaf(l).unwrap().active, Some(a));
}

// 6 ──────────────────────────────────────────────────────────────────
#[test]
fn doc_released_only_with_last_tab_and_flush_only_for_last() {
    let (mut ws, a, b, _) = setup("x");
    ws.edit(a, ins(1, "y")).unwrap(); // document is dirty

    // not the last tab: no flush needed
    assert_eq!(ws.request_close(a).unwrap(), CloseRequest::Ready);
    ws.finish_close(a).unwrap();
    assert_eq!(ws.docs.len(), 1);

    // last one: FlushDoc needed, without it finish_close refuses
    let CloseRequest::NeedsEffects(fx) = ws.request_close(b).unwrap() else { panic!() };
    assert!(matches!(fx.as_slice(), [Effect::FlushDoc(_)]));
    assert_eq!(ws.finish_close(b), Err(WsError::PendingEffects));
    ws.run_effect(&fx[0]).unwrap();
    ws.finish_close(b).unwrap();
    assert_eq!(ws.docs.len(), 0);
}

// 7 ──────────────────────────────────────────────────────────────────
#[test]
fn split_of_same_file_shares_one_document_with_two_sessions() {
    let (mut ws, a, b, doc) = setup("one two");
    assert_eq!(ws.docs.len(), 1);
    assert_eq!(ws.tabs.tabs_of(doc), vec![a, b]);
    set_cursor(&mut ws, b, 7, None);

    ws.edit(a, ins(0, ">> ")).unwrap();

    assert_eq!(ws.docs.get(doc).unwrap().content(), ">> one two");
    assert_eq!(ws.tabs.get(b).unwrap().cursor.offset, 10);
}

// 8 ──────────────────────────────────────────────────────────────────
#[test]
fn resizing_one_leaf_does_not_invalidate_neighbour_cache() {
    let (mut ws, a, b, _) = setup("x");
    let (l_a, l_b) = (ws.layout.leaf_of(a).unwrap(), ws.layout.leaf_of(b).unwrap());
    assert_ne!(l_a, l_b);
    assert_eq!((ws.refresh_cache(a), ws.refresh_cache(b)), (Ok(true), Ok(true)));
    assert_eq!((ws.refresh_cache(a), ws.refresh_cache(b)), (Ok(false), Ok(false)));

    ws.set_leaf_bounds(l_a, Rect { w: 800.0, h: 300.0 }).unwrap();

    assert_eq!(ws.refresh_cache(a), Ok(true));
    assert_eq!(ws.refresh_cache(b), Ok(false));
    assert_eq!(ws.cache(b).unwrap().rebuilds, 1);
}

#[test]
fn edit_invalidates_cache_of_every_tab_of_the_doc() {
    let (mut ws, a, b, _) = setup("x");
    ws.refresh_cache(a).unwrap();
    ws.refresh_cache(b).unwrap();
    ws.edit(a, ins(0, "q")).unwrap();
    assert_eq!((ws.refresh_cache(a), ws.refresh_cache(b)), (Ok(true), Ok(true)));
}

// 9 ──────────────────────────────────────────────────────────────────
#[test]
fn rename_is_visible_in_all_tabs_and_path_index_follows() {
    let (mut ws, a, b, doc) = setup("x");
    ws.docs.rename(doc, "Renamed").unwrap();
    assert_eq!(ws.title_of(a), Some("Renamed"));
    assert_eq!(ws.title_of(b), Some("Renamed"));
    assert_eq!(ws.docs.id_by_path(std::path::Path::new("/v/renamed.md")), Some(doc));
    assert_eq!(ws.docs.id_by_path(std::path::Path::new("/v/a.md")), None);
}

// extras ─────────────────────────────────────────────────────────────
#[test]
fn path_normalisation_keeps_one_document_per_file() {
    let mut reg = DocumentRegistry::new();
    let x = reg.open(std::path::Path::new("D:\\Vault\\A.md"), "x");
    let y = reg.open(std::path::Path::new("d:/vault/a.md"), "y");
    assert_eq!(x, y);
    assert_eq!(reg.len(), 1);
}

#[test]
fn multi_op_event_applies_sequentially() {
    let mut store = TabSessionStore::new();
    let doc = DocumentId(0);
    let mut s = TabSession::new(TabId(1), doc);
    s.cursor.offset = 10;
    store.insert(s);
    let ev = EditEvent {
        doc,
        version: 1,
        ops: vec![TextOp::Delete { at: 2, len: 3 }, TextOp::Insert { at: 2, len: 1 }],
        origin: None,
        cursor_after: None,
    };
    store.apply_edit_event(&ev);
    // 10 → 7 (after Delete) → 8 (after Insert)
    assert_eq!(store.get(TabId(1)).unwrap().cursor.offset, 8);
}

#[test]
fn external_change_with_no_origin_transforms_every_tab() {
    let (mut ws, a, b, doc) = setup("abc");
    set_cursor(&mut ws, a, 3, None);
    set_cursor(&mut ws, b, 3, None);
    let ev = ws.docs.apply_edit(doc, ins(0, "__"), None).unwrap();
    ws.tabs.apply_edit_event(&ev);
    assert_eq!(ws.tabs.get(a).unwrap().cursor.offset, 5);
    assert_eq!(ws.tabs.get(b).unwrap().cursor.offset, 5);
}

#[test]
fn bad_range_is_rejected_without_changing_document() {
    let (mut ws, a, _b, doc) = setup("abc");
    assert_eq!(ws.edit(a, EditCmd::Delete { at: 2, len: 5 }), Err(WsError::BadRange));
    assert_eq!(ws.edit(a, ins(9, "z")), Err(WsError::BadRange));
    assert_eq!(ws.docs.get(doc).unwrap().content(), "abc");
    assert_eq!(ws.docs.get(doc).unwrap().version, 0);
}

#[test]
fn transform_offset_edge_cases() {
    use TextOp::*;
    assert_eq!(transform_offset(5, Insert { at: 5, len: 2 }), 7); // insertion at the cursor position shifts
    assert_eq!(transform_offset(5, Insert { at: 6, len: 2 }), 5);
    assert_eq!(transform_offset(5, Delete { at: 3, len: 4 }), 3); // cursor inside the deleted range → moves to the start
    assert_eq!(transform_offset(7, Delete { at: 3, len: 4 }), 3);
    assert_eq!(transform_offset(8, Delete { at: 3, len: 4 }), 4);
}
