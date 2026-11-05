use super::*;

impl AppView {
    pub(crate) fn render_central_editor(
        &self,
        override_view: Option<&crate::workspace::EditorViewState>,
        override_doc: Option<&crate::document_buffer::DocumentBuffer>,
        is_leaf_active: bool,
        leaf_id: Option<crate::workspace::LeafId>,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let editor_font = self.editor_font_family();
        let typography = self.editor_typography();
        let line_char_width = self.editor_char_width(cx);

        crate::images::set_char_width(line_char_width);
        crate::images::set_folder(&self.config.files.folder);
        crate::images::set_default_corner_radius(self.config.images.corner_radius as f32);

        let pixel_scale = window.scale_factor();
        let window_h = f32::from(window.bounds().size.height);
        // The sidebar inline rename holds focus while it is open: the editor must
        // not paint its caret or current-line highlight during that time.
        let sidebar_rename_focused = self.bookmark_rename_focus_handle.is_focused(window);

        let root_id = if let Some(lid) = leaf_id {
            format!("central-editor-root-{}", lid.0)
        } else {
            "central-editor-root".to_string()
        };

        let mut editor_root = div()
            .id(gpui::ElementId::Name(root_id.into()))
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(theme.bg_editor);

        if !is_leaf_active {
            if let Some(lid) = leaf_id {
                editor_root = editor_root
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        this.switch_to_leaf(lid, window, cx);
                        window.focus(&this.focus_handle);
                        cx.notify();
                    }))
                    .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| {
                        this.switch_to_leaf(lid, window, cx);
                        this.start_editor_middle_scroll(cx);
                    }));
            }
        }

        editor_root = editor_root.child(self.render_navigation_bar(override_doc, leaf_id, theme, cx));

        let window_w: f32 = window.bounds().size.width.into();
        let editor_left_w = if self.sidebar_collapsed {
            44.0
        } else {
            self.sidebar_width + 1.0
        };
        let editor_right_w = if self.right_sidebar_collapsed {
            0.0
        } else {
            self.right_sidebar_width + 1.0
        };
        let workspace_w = (window_w - editor_left_w - editor_right_w).max(100.0);
        let in_titlebar = leaf_id.map(|lid| self.state.workspace.is_leaf_in_titlebar(lid)).unwrap_or(false);
        let leaf_h_ratio = leaf_id.map(|lid| self.state.workspace.horizontal_ratio_of_leaf(lid)).unwrap_or(0.0);
        let leaf_w_ratio = leaf_id.map(|lid| self.state.workspace.horizontal_width_ratio_of_leaf(lid)).unwrap_or(1.0);
        let leaf_v_ratio = leaf_id.map(|lid| self.state.workspace.vertical_ratio_of_leaf(lid)).unwrap_or(0.0);

        let toolbar_origin_x = editor_left_w + (workspace_w * leaf_h_ratio);
        let toolbar_width = (workspace_w * leaf_w_ratio).max(100.0);

        let workspace_h = (window_h - 66.0).max(100.0);
        let tab_bar_h = if in_titlebar { 0.0 } else { 36.0 };
        let nav_bar_h = 40.0;
        let titlebar_h = 38.0;
        let toolbar_origin_y = titlebar_h + (workspace_h * leaf_v_ratio) + tab_bar_h + nav_bar_h;

        if let Some(top_toolbar) = self.render_toolbar_strips(
            crate::toolbar::ToolbarPlacement::Top,
            toolbar_origin_x,
            toolbar_origin_y,
            toolbar_width,
            theme,
            cx,
        ) {
            editor_root = editor_root.child(top_toolbar);
        }

        let current_view_and_doc = match (override_view, override_doc) {
            (Some(v), Some(d)) => Some((v, d)),
            _ if is_leaf_active && self.active_center_leaf.is_none() => {
                self.state.active_view_and_doc()
            }
            _ => None,
        };

        if let Some((view, doc)) = current_view_and_doc {
            const NOTE_EDGE_PAD: f32 = 32.0;
            let (note_margin_left, note_margin_right) = self.note_margins_for_leaf(leaf_id, window_w);
            let width_limited = self.config.editor.editor_width_pct < 100;
            let show_zones = self.config.editor.show_width_limit_zones;
            let show_left_band = show_zones && width_limited && note_margin_left > 0.0;
            let show_right_band = show_zones && width_limited && note_margin_right > 0.0;
            // The current-line wash is a child of a row inside the padded note
            // column. Negative insets pull it over the gutters, but a band on
            // that side stops it at the note column instead.
            let highlight_left = if show_left_band { 0.0 } else { -(NOTE_EDGE_PAD + note_margin_left) };
            let highlight_right = if show_right_band { 0.0 } else { -(NOTE_EDGE_PAD + note_margin_right) };

            let note_title = doc
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".to_string());

            let lines_arc = Arc::clone(&view.window.lines);
            let doc_lines = doc.line_index.lines();
            // The gutter itself owns this width: long numbers grow it instead
            // of spilling left into the padding and under the width-limit band.
            let line_number_width = Self::line_number_gutter_width(doc_lines);
            let visual_rows_arc = Arc::clone(&view.window.visual_rows);
            let code_states_arc = Arc::clone(&view.window.code_states);
            let height_index_arc = Arc::clone(&view.window.height_index);

            // Guide columns of every list block in the window. Code rows are
            // skipped so a fenced example cannot shape the step of a neighbouring
            // list.
            let list_guide_columns = crate::markdown::list_block_guide_columns(
                lines_arc
                    .as_slice()
                    .iter()
                    .enumerate()
                    .map(|(idx, line)| {
                        code_states_arc
                            .get(lines_arc.start() + idx)
                            .map_or(true, |state| state.is_none())
                            .then_some(line.as_str())
                    }),
            );

            // Callout extents for the window, so every row of a block is tinted and
            // only the two ends are rounded. The two windows share a start line.
            let callout_blocks = crate::markdown::callout_blocks(
                lines_arc.as_slice(),
                code_states_arc.as_slice(),
            );

            let (cursor_line, cursor_col) = view.line_col(doc);
            let (active_v_idx, active_col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows_arc, cursor_line, cursor_col)
                .unwrap_or((0, 0));

            let selection_range = view.selection_range(doc.len());
            let is_editing = is_leaf_active && self.edit_mode && !self.ctrl_preview_selection;
            // Lines marked by the find bar's "Mark all" keep a narrow column
            // free at the right edge of the gutter, between the number and the
            // text, for the red arrow.
            const FIND_MARKER_W: f32 = 10.0;
            let markers_active = self.find_bar.marks_valid_for(doc.id, doc.version);
            // While a find field owns the keyboard the editor is not the focus
            // target, so the active leaf draws its caret the way an unfocused
            // split pane does: thin, muted and static.
            let find_field_focused = self.find_bar.is_open
                && (self.find_bar.query.is_focused(window)
                    || self.find_bar.replace.is_focused(window));
            // The inline rename takes the keyboard away from the editor, so the
            // editor renders the same way an unfocused split pane does: a static
            // muted caret and a dashed current-line frame.
            let caret_is_inactive =
                !is_leaf_active || find_field_focused || sidebar_rename_focused;
            let mut active_block_ids: std::collections::HashSet<usize> = std::collections::HashSet::new();
            if is_editing {
                if let Some(s) = code_states_arc.get(cursor_line).and_then(|s| s.as_ref()) {
                    active_block_ids.insert(s.block_id);
                }
                if let Some(ref sel) = selection_range {
                    if sel.start < sel.end {
                        let sel_start_line = doc.line_of_byte(sel.start);
                        let sel_end_line = doc.line_of_byte(sel.end);
                        let win_start = code_states_arc.start();
                        let win_end = code_states_arc.end();
                        let scan_start = sel_start_line.max(win_start);
                        let scan_end = sel_end_line.min(win_end.saturating_sub(1));
                        if scan_start <= scan_end {
                            for l_idx in scan_start..=scan_end {
                                if let Some(Some(s)) = code_states_arc.get(l_idx) {
                                    active_block_ids.insert(s.block_id);
                                }
                            }
                        }
                    }
                }
            }

            let text_start_x = self.pane_text_start_x(leaf_id, window);

            let theme_scrollbar = theme.clone();

            let total_content_h = view.total_height();
            let (slice_h, viewport_h, max_scroll_content, track_top_y) =
                self.editor_scroll_metrics_for_leaf(leaf_id, is_leaf_active, total_content_h, window_h);
            if is_leaf_active {
                self.last_editor_h.set(viewport_h);
            }

            let wide_scrollbar = self.config.editor.wide_scrollbar;
            let show_scrollbar = max_scroll_content > 1.0;

            let leaf_scroll_top = if is_leaf_active {
                if let Some(target) = self.pending_editor_restore {
                    target
                } else if self.is_resizing_any() && view.window.covers_line(view.viewport.offset) {
                    let first_v_idx = view.window.first_row_of_line(view.viewport.offset).unwrap_or(0);
                    let line_y = view.above_h + view.window.height_index.top_of(first_v_idx);
                    let curr = self.current_scroll_top_f64();
                    if (curr - line_y).abs() > (view.window.total_height() * 0.4).max(500.0) {
                        line_y
                    } else {
                        curr
                    }
                } else {
                    self.current_scroll_top_f64()
                }
            } else {
                leaf_id.and_then(|lid| {
                    self.state.leaves.get(&lid)
                        .and_then(|l| l.active_tab())
                        .and_then(|t| self.state.views.get(&t.id))
                        .map(|v| v.scroll_top)
                }).unwrap_or(0.0)
            };

            let (thumb_top, thumb_h) = if show_scrollbar {
                let th = Self::editor_thumb_h(viewport_h, total_content_h, wide_scrollbar);
                let scroll_y = leaf_scroll_top.clamp(0.0, max_scroll_content);
                let scroll_fraction = (scroll_y / max_scroll_content).clamp(0.0, 1.0);
                let tt = scroll_fraction * (f64::from(viewport_h) - th);
                (tt as f32, th as f32)
            } else {
                (0.0, 28.0)
            };

            let is_resizing = self.is_resizing_any();
            let is_scrollbar_dragging = is_leaf_active && self.is_dragging_scrollbar && !is_resizing;
            let scroll_y_current = leaf_scroll_top.clamp(0.0, max_scroll_content);
            let ruler_expanded = is_leaf_active && self.is_hovering_editor_scrollbar && !is_resizing;
            let track_w = if wide_scrollbar { 16.0 } else { 14.0 };
            let thumb_right = if wide_scrollbar { 0.0 } else { 3.0 };
            let thumb_w_idle = if wide_scrollbar { track_w } else { 6.0 };
            let thumb_w_active = if wide_scrollbar { track_w } else { 8.0 };
            let thumb_w = if is_scrollbar_dragging { thumb_w_active } else { thumb_w_idle };
            let thumb_left = track_w - thumb_right - thumb_w;
            let ruler_pill_w = if is_scrollbar_dragging { thumb_w } else { 6.0 };
            let ruler_pill_left = thumb_left + (thumb_w - ruler_pill_w) / 2.0;
            let ruler_full = ruler_expanded || wide_scrollbar;
            let thumb_bg = if wide_scrollbar {
                theme_scrollbar.border
            } else if is_scrollbar_dragging {
                theme_scrollbar.accent
            } else {
                theme_scrollbar.border
            };

            let show_ruler = self.config.editor.scrollbar_ruler;
            let (ruler_cursor_y, ruler_sel_span): (f32, Option<(f32, f32)>) =
                if show_ruler && show_scrollbar && total_content_h > 1.0 {
                    let to_track = |doc_y: f64| {
                        ((doc_y / total_content_h) * f64::from(viewport_h))
                            .clamp(0.0, f64::from(viewport_h) - 2.0)
                            as f32
                    };
                    let lines_total = doc.line_index.lines();
                    let cursor_doc_y = if view.window.covers_line(cursor_line) {
                        view.doc_y_of_local_v_idx(active_v_idx)
                    } else {
                        view.doc_y_of_line(lines_total, cursor_line)
                    };
                    let cursor_y = to_track(cursor_doc_y);
                    let sel_span = view.selection_range(doc.len()).map(|sel| {
                        let start_line = doc.line_of_byte(sel.start);
                        let end_line = doc.line_of_byte(sel.end);
                        let a = view.doc_y_of_line(lines_total, start_line);
                        let b = view.doc_y_of_line(lines_total, end_line);
                        let top = to_track(a.min(b));
                        let bottom = to_track(a.max(b)) + 2.0;
                        (top, (bottom - top).max(3.0))
                    });
                    (cursor_y, sel_span)
                } else {
                    (0.0, None)
                };

            let ruler_cursor_under_thumb = show_scrollbar
                && !wide_scrollbar
                && (is_scrollbar_dragging || (is_leaf_active && self.is_hovering_editor_thumb))
                && ruler_cursor_y < thumb_top + thumb_h
                && ruler_cursor_y + 2.0 > thumb_top;

            let leaf_active_tab_id = leaf_id.and_then(|lid| {
                self.state.leaves.get(&lid).and_then(|l| l.active_tab()).map(|t| t.id)
            });
            let leaf_view = leaf_active_tab_id.and_then(|tid| self.state.views.get(&tid));
            let title_input_opt = leaf_view.and_then(|v| v.title_input.as_ref());
            let is_editing_title = title_input_opt.is_some();
            let is_title_focused = is_leaf_active && is_editing_title && self.title_focus_handle.is_focused(window);

            let title_text: String = if let Some(input) = title_input_opt {
                input.text.clone()
            } else {
                note_title.clone()
            };
            let title_chars: Vec<char> = title_text.chars().collect();
            let cursor_pos = if let Some(input) = title_input_opt {
                input.cursor_pos.min(title_chars.len())
            } else {
                0
            };
            let title_selection_range = if is_leaf_active {
                title_input_opt.and_then(|i| i.selection_range())
            } else {
                None
            };

            let run = gpui::TextRun {
                len: title_text.len(),
                font: gpui::font("Segoe UI").bold(),
                color: Default::default(),
                underline: Default::default(),
                strikethrough: None,
                background_color: None,
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(title_text.clone()),
                px(28.0),
                &[run],
                None,
            );

            let cursor_x = if title_text.is_empty() {
                0.0
            } else {
                let byte_index: usize = title_chars[..cursor_pos].iter().map(|c| c.len_utf8()).sum();
                crate::snap::snap_x(
                    f32::from(shaped.x_for_index(byte_index)),
                    pixel_scale,
                )
            };

            let (sel_x1, sel_w) = if let Some((s_start, s_end)) = title_selection_range {
                if !title_text.is_empty() {
                    let b1: usize = title_chars[..s_start.min(title_chars.len())].iter().map(|c| c.len_utf8()).sum();
                    let b2: usize = title_chars[..s_end.min(title_chars.len())].iter().map(|c| c.len_utf8()).sum();
                    // Same device-pixel snapping as the caret above: an
                    // unsnapped antialiased edge shifts its blend ratio with
                    // every sub-pixel move and reads as a stuttering edge.
                    crate::snap::snapped_span(
                        f32::from(shaped.x_for_index(b1)),
                        f32::from(shaped.x_for_index(b2)),
                        pixel_scale,
                    )
                } else {
                    (0.0, 0.0)
                }
            } else {
                (0.0, 0.0)
            };

            let display_title = if title_text.is_empty() {
                if is_title_focused {
                    String::new()
                } else {
                    note_title.clone()
                }
            } else {
                title_text.clone()
            };

            let lid_tag: u64 = leaf_id.map(|l| (l.0.as_u128() & 0xFFFF_FFFF_FFFF_FFFF) as u64).unwrap_or(0);

            let window_w_title = f32::from(window.bounds().size.width);
            let title_text_w = f32::from(shaped.width);
            let title_left_offset = self.note_title_offset_for_leaf(leaf_id, window_w_title, title_text_w);

            let mut title_box = div()
                .id(gpui::ElementId::from(("note-title-panel", lid_tag)))
                .w_full()
                .flex()
                .flex_row()
                .items_start()
                .pl(px(title_left_offset))
                .pr(px(16.0))
                .pt(px(2.0))
                .pb(px(16.0))
                .cursor(gpui::CursorStyle::IBeam)
                .bg(theme.bg_editor);

            if is_leaf_active {
                title_box = title_box
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        window.focus(&this.title_focus_handle);
                        if let Some((view, doc)) = this.state.active_view_and_doc_mut() {
                            if view.title_input.is_none() {
                                view.start_title_edit(doc.title.clone());
                            }
                            if let Some(ref mut input) = view.title_input {
                                input.is_focused = true;
                                input.cursor_pos = input.text.chars().count();
                                input.selection_anchor = None;
                                input.is_mouse_selecting = false;
                            }
                        }
                        this.reset_title_cursor_blink(cx);
                        cx.notify();
                    }));
            } else if let Some(lid) = leaf_id {
                title_box = title_box
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        this.switch_to_leaf(lid, window, cx);
                        window.focus(&this.focus_handle);
                        cx.notify();
                    }));
            }

            let mut title_input = div()
                .id(gpui::ElementId::from(("note-title-input", lid_tag)))
                .relative()
                .flex_1()
                .min_w_0()
                .h(px(36.0))
                .cursor(gpui::CursorStyle::IBeam)
                .flex()
                .flex_row()
                .items_center();

            if is_leaf_active {
                title_input = title_input
                    .track_focus(&self.title_focus_handle)
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        window.focus(&this.title_focus_handle);
                        this.reset_title_cursor_blink(cx);

                        let click_x: f32 = event.position.x.into();
                        let editor_left_x = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                        let editor_right_x = if this.right_sidebar_collapsed { 0.0 } else { this.right_sidebar_width + 1.0 };
                        let leaf_ratio = leaf_id.map(|lid| this.state.workspace.horizontal_ratio_of_leaf(lid)).unwrap_or(0.0);
                        let window_w: f32 = window.bounds().size.width.into();
                        let workspace_w = (window_w - editor_left_x - editor_right_x).max(100.0);
                        let title_left_x = editor_left_x + leaf_ratio * workspace_w + title_left_offset;
                        let rel_x = (click_x - title_left_x).max(0.0);

                        let Some((view, doc)) = this.state.active_view_and_doc_mut() else { return; };
                        if view.title_input.is_none() {
                            view.start_title_edit(doc.title.clone());
                        }
                        let Some(ref mut input) = view.title_input else { return; };
                        input.is_focused = true;

                        let clicked_pos = if input.text.is_empty() {
                            0
                        } else {
                            let run = gpui::TextRun {
                                len: input.text.len(),
                                font: gpui::font("Segoe UI").bold(),
                                color: Default::default(),
                                underline: Default::default(),
                                strikethrough: None,
                                background_color: None,
                            };
                            let shaped = window.text_system().shape_line(
                                SharedString::from(input.text.clone()),
                                px(28.0),
                                &[run],
                                None,
                            );
                            let byte_idx = shaped.closest_index_for_x(px(rel_x));
                            input.text[..byte_idx.min(input.text.len())].chars().count()
                        };

                        if event.click_count == 2 {
                            let (s, e) = crate::views::goto_search::word_range_at(&input.text, clicked_pos);
                            input.selection_anchor = Some(s);
                            input.cursor_pos = e;
                            input.is_mouse_selecting = false;
                        } else if event.click_count >= 3 {
                            input.select_all();
                            input.is_mouse_selecting = false;
                        } else if event.modifiers.shift {
                            if input.selection_anchor.is_none() {
                                input.selection_anchor = Some(input.cursor_pos);
                            }
                            input.cursor_pos = clicked_pos;
                            input.is_mouse_selecting = true;
                        } else {
                            input.cursor_pos = clicked_pos;
                            input.selection_anchor = Some(clicked_pos);
                            input.is_mouse_selecting = true;
                        }
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.handle_title_key(event, window, cx);
                    }));
            } else if let Some(lid) = leaf_id {
                title_input = title_input
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.switch_to_leaf(lid, window, cx);
                        window.focus(&this.focus_handle);
                        cx.notify();
                    }));
            }

            title_input = title_input
                .when(title_selection_range.is_some() && sel_w > 0.0, |s| {
                    s.child(
                        div()
                            .absolute()
                            .left(px(sel_x1))
                            .top(px(4.0))
                            .w(px(sel_w))
                            .h(px(28.0))
                            .bg(theme.selection_bg)
                            .rounded_xs(),
                    )
                })
                .child(
                    div()
                        .font_family("Segoe UI")
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_size(px(28.0))
                        .line_height(px(36.0))
                        .text_color(rgb(0xf44336))
                        .child(display_title),
                )
                .when(title_text.is_empty() && is_title_focused, |s| {
                    s.child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .h(px(36.0))
                            .font_family("Segoe UI")
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_size(px(28.0))
                            .line_height(px(36.0))
                            .text_color(rgba(0xf4433666))
                            .child("Untitled"),
                    )
                })
                .when(is_title_focused && self.title_blink.visible(), |s| {
                    s.child(
                        div()
                            .absolute()
                            .left(px(cursor_x))
                            .top(px(5.0))
                            .w(px(2.0))
                            .h(px(26.0))
                            .bg(rgb(0xf44336))
                            .rounded_xs(),
                    )
                });

            title_box = title_box.child(title_input);

            let image_drag_bg = theme.bg_hover;
            let mut editor_container = div()
                .id(gpui::ElementId::from(("note-editor-container", lid_tag)))
                .relative()
                .flex()
                .flex_col()
                .flex_1()
                .w_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden();

            if is_leaf_active {
                editor_container = editor_container
                    .track_focus(&self.focus_handle)
                    .cursor(gpui::CursorStyle::IBeam)
                    .when(!show_scrollbar, |d| {
                        d.on_scroll_wheel(|_, _, cx| {
                            cx.stop_propagation();
                        })
                    })
                    // Files from Explorer: import images next to the note and
                    // insert the reference where the pointer was released.
                    .drag_over::<std::sync::Arc<Vec<PathBuf>>>(move |style, _, _, _| {
                        style.bg(image_drag_bg)
                    })
                    .on_drop::<std::sync::Arc<Vec<PathBuf>>>(
                        cx.listener(|this, paths: &std::sync::Arc<Vec<PathBuf>>, window, cx| {
                            let position = window.mouse_position();
                            let x: f32 = position.x.into();
                            let y: f32 = position.y.into();
                            this.is_mouse_selecting = false;
                            this.handle_editor_mouse_down(x, y, false, false, 1, window, cx);
                            let files: Vec<PathBuf> = paths
                                .iter()
                                .filter(|path| crate::images::is_image_path(path))
                                .cloned()
                                .collect();
                            if files.is_empty() {
                                this.push_notification(
                                    crate::views::notification::NotificationType::Info,
                                    None,
                                    crate::i18n::t("editor.image.drop_not_image", &this.config.ui.language),
                                    Some(5000),
                                    cx,
                                );
                            } else {
                                this.import_image_files(files, cx);
                            }
                        }),
                    )
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        if this.is_resizing_any() {
                            return;
                        }
                        this.close_editor_context_menu(cx);
                        this.commit_title_rename(window, cx);
                        window.focus(&this.focus_handle);
                        this.is_mouse_selecting = true;
                        let click_x: f32 = event.position.x.into();
                        let click_y: f32 = event.position.y.into();
                        this.mouse_down_pos = Some((click_x, click_y));
                        this.mouse_drag_started = false;
                        this.handle_editor_mouse_down(click_x, click_y, event.modifiers.shift, event.modifiers.alt, event.click_count, window, cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        if this.is_resizing_any() {
                            return;
                        }
                        this.commit_title_rename(window, cx);
                        window.focus(&this.focus_handle);
                        let click_x: f32 = event.position.x.into();
                        let click_y: f32 = event.position.y.into();
                        let has_selection = this.state.active_view()
                            .and_then(|v| this.state.active_document().and_then(|d| v.selection_range(d.len())))
                            .map(|r| r.start < r.end)
                            .unwrap_or(false);
                        if !has_selection {
                            this.handle_editor_mouse_down(click_x, click_y, false, false, 1, window, cx);
                            if let Some(view) = this.state.active_view_mut() {
                                view.selection_anchor = None;
                            }
                        }
                        this.open_editor_context_menu(click_x, click_y, cx);
                    }))
                    .on_mouse_down(MouseButton::Middle, cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        if this.is_resizing_any() {
                            return;
                        }
                        cx.stop_propagation();
                        window.focus(&this.focus_handle);
                        this.close_editor_context_menu(cx);
                        this.is_mouse_selecting = false;
                        this.start_editor_middle_scroll(cx);
                    }))
                    .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                        if this.is_resizing_sidebar {
                            let x: f32 = event.position.x.into();
                            if x < 100.0 {
                                this.sidebar_collapsed = true;
                            } else {
                                this.sidebar_collapsed = false;
                                this.sidebar_width = x.clamp(160.0, 600.0);
                            }
                            cx.notify();
                        } else if this.is_resizing_right_sidebar {
                            let win_w: f32 = window.viewport_size().width.into();
                            let x: f32 = event.position.x.into();
                            let right_w = win_w - x;
                            if right_w < 100.0 {
                                this.right_sidebar_collapsed = true;
                            } else {
                                this.right_sidebar_collapsed = false;
                                this.right_sidebar_width = right_w.clamp(160.0, 600.0);
                            }
                            cx.notify();
                        } else if this.split_drag_state.is_some() {
                            return;
                        } else if this.editor_middle_active {
                            let y: f32 = event.position.y.into();
                            if let Some((top, h, max)) = this.editor_middle_metrics() {
                                this.on_editor_middle_mouse_move(y, top, h, max, cx);
                            } else {
                                cx.notify();
                            }
                        } else if this.is_dragging_scrollbar {
                            let y: f32 = event.position.y.into();
                            this.handle_scrollbar_drag(y, window, cx);
                        } else if let Some(drag) = this.table_col_resize_drag.clone() {
                            let x: f32 = event.position.x.into();
                            let delta = x - drag.start_x;
                            // A table may not grow past the note column: otherwise its
                            // right edge runs under the pale width-limit band or off the
                            // pane edge instead of stopping at it.
                            let win_w: f32 = window.viewport_size().width.into();
                            let char_w = if line_char_width > 1.0 { line_char_width } else { 7.7 };
                            let room = (this.compute_wrap_cols_for_leaf(leaf_id, win_w, line_char_width)
                                as f32
                                * char_w)
                                .max(160.0);
                            let other_widths: f32 = this
                                .manual_table_col_widths
                                .get(&drag.table_first_line)
                                .map(|widths| {
                                    widths
                                        .iter()
                                        .enumerate()
                                        .filter(|(idx, _)| *idx != drag.col_idx)
                                        .map(|(_, w)| *w)
                                        .sum()
                                })
                                .unwrap_or(0.0);
                            let max_w = (room - other_widths).max(40.0);
                            let new_w = (drag.start_width + delta).clamp(40.0, max_w);
                            if let Some(widths) = this.manual_table_col_widths.get_mut(&drag.table_first_line) {
                                if drag.col_idx < widths.len() {
                                    widths[drag.col_idx] = new_w;
                                }
                            }
                            this.table_layouts.borrow_mut().remove(&drag.table_first_line);
                            cx.notify();
                        } else if this.is_dragging_note_width_slider {
                            let x: f32 = event.position.x.into();
                            let track_x = this.note_width_slider_origin_x.get();
                            let rel_x = x - track_x;
                            let new_pct = ((rel_x / 120.0) * 100.0).round().clamp(0.0, 100.0) as u32;
                            this.set_editor_width_pct(new_pct, window, cx);
                        } else if this.is_mouse_selecting {
                            let x: f32 = event.position.x.into();
                            let y: f32 = event.position.y.into();
                            let viewport_h: f32 = window.viewport_size().height.into();
                            this.update_mouse_selection(x, y, viewport_h, window, cx);
                        }
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.is_resizing_sidebar = false;
                        this.is_resizing_right_sidebar = false;
                        this.split_drag_state = None;
                        this.hovered_split_divider = None;
                        this.is_dragging_scrollbar = false;
                        this.sidebar_scrollbar_drag.end();
                        this.recent_files_scrollbar_drag.end();
                        this.plugin_modal_scrollbar_drag.end();
                        this.is_mouse_selecting = false;
                        this.mouse_drag_started = false;
                        this.mouse_down_pos = None;
                        if this.is_dragging_note_width_slider {
                            this.is_dragging_note_width_slider = false;
                            this.save_settings(cx);
                        }
                        this.table_col_resize_drag = None;
                        if let Some(view) = this.state.active_view_mut() {
                            if view.selection_anchor == Some(view.cursor_offset) {
                                view.selection_anchor = None;
                            }
                        }
                        cx.notify();
                    }))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.is_resizing_sidebar = false;
                        this.is_resizing_right_sidebar = false;
                        this.split_drag_state = None;
                        this.hovered_split_divider = None;
                        this.is_dragging_scrollbar = false;
                        this.sidebar_scrollbar_drag.end();
                        this.recent_files_scrollbar_drag.end();
                        this.plugin_modal_scrollbar_drag.end();
                        this.is_mouse_selecting = false;
                        this.mouse_drag_started = false;
                        this.mouse_down_pos = None;
                        if this.is_dragging_note_width_slider {
                            this.is_dragging_note_width_slider = false;
                            this.save_settings(cx);
                        }
                        this.table_col_resize_drag = None;
                        cx.notify();
                    }))
                    .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                        this.stop_editor_middle_scroll(cx);
                        this.stop_plugin_modal_middle_scroll(cx);
                    }))
                    .on_mouse_up_out(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                        this.stop_editor_middle_scroll(cx);
                        this.stop_plugin_modal_middle_scroll(cx);
                    }))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        this.handle_key(event, window, cx);
                    }))
                    .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _window, cx| {
                        let ctrl_held = event.modifiers.control || event.modifiers.platform;
                        if this.ctrl_held != ctrl_held {
                            this.ctrl_held = ctrl_held;
                            cx.notify();
                        }
                    }));
            }

            let headings = &typography.headings;
            let heading_widths = Self::resolve_heading_char_widths(cx.text_system(), &editor_font, &typography, line_char_width);
            let text_system = window.text_system().clone();
            let hide_current_line_on_selection = self.config.editor.hide_current_line_on_selection;
            let has_selection = selection_range.as_ref().map_or(false, |sel| sel.start < sel.end);

            let (visible_range, slice_shift) = if !show_scrollbar {
                if is_leaf_active && self.current_scroll_top_f64() > 0.0 {
                    self.set_editor_scroll_top_f64(0.0);
                }
                (0..visual_rows_arc.len(), 0.0)
            } else {
                let raw_scroll_y = leaf_scroll_top;
                let true_max = max_scroll_content
                    .max(total_content_h - f64::from(viewport_h))
                    .max(0.0);
                let scroll_y = if self.is_resizing_any() {
                    raw_scroll_y.max(0.0)
                } else {
                    raw_scroll_y.clamp(0.0, true_max)
                };
                if is_leaf_active && !self.is_resizing_any() && (raw_scroll_y - scroll_y).abs() > 0.5 {
                    self.set_editor_scroll_top_f64(scroll_y);
                }
                let local_scroll = scroll_y - view.above_h;
                let vr = height_index_arc.visible_range(
                    local_scroll,
                    f64::from(slice_h),
                    f64::from(EDITOR_OVERSCAN_PX),
                );
                let shift = height_index_arc.top_of(vr.start) - local_scroll;
                let shift = crate::snap::snap_x(shift as f32, pixel_scale) as f64;
                (vr, shift)
            };

            let (slice_first_line, slice_line_starts) = {
                let lo = visible_range.start.saturating_sub(1);
                let hi = (visible_range.end + 1).min(visual_rows_arc.len());
                if lo >= hi {
                    (0, Vec::new())
                } else {
                    let first = visual_rows_arc[lo].line_idx;
                    let last = visual_rows_arc[hi - 1].line_idx;
                    let mut starts = Vec::with_capacity(last - first + 1);
                    let mut at = doc.line_start_byte(first);
                    for l_idx in first..=last {
                        starts.push(at);
                        at += lines_arc[l_idx].len() + 1;
                    }
                    (first, starts)
                }
            };

            let mut items = Vec::with_capacity(visible_range.len());

            let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
            let calc_v_row_sel_rect = |target_v_idx: usize| -> Option<(f32, f32)> {
                let sel = selection_range.as_ref()?;
                if sel.start >= sel.end {
                    return None;
                }
                let v = visual_rows_arc.get(target_v_idx)?;
                let line_idx = v.line_idx;
                if line_idx < slice_first_line || line_idx - slice_first_line >= slice_line_starts.len() {
                    return None;
                }
                let line = &lines_arc[line_idx];
                let line_start = slice_line_starts[line_idx - slice_first_line];
                let line_end = line_start + line.len();

                let is_line_in_sel = if sel.start == sel.end {
                    false
                } else if line_start == line_end {
                    sel.start <= line_start && sel.end > line_start
                } else {
                    sel.end > line_start && sel.start < line_end
                };
                let is_in_act_block = code_states_arc.get(line_idx)
                    .and_then(|s| s.as_ref())
                    .map(|s| active_block_ids.contains(&s.block_id))
                    .unwrap_or(false);
                let is_act_line = is_editing && (line_idx == cursor_line || is_line_in_sel || is_in_act_block);
                let code_state = code_states_arc.get(line_idx).and_then(|s| s.as_ref()).map(|s| (s.kind, s.lang.as_deref()));
                let classified = crate::markdown::classify_line(line, code_state);
                let reveal = if is_editing {
                    crate::markdown::inline_reveal(
                        line,
                        &classified,
                        (cursor_line == line_idx).then_some(cursor_col),
                        selection_chars_in_line(line, line_start, selection_range.as_ref()),
                        view.reveal_all_line == Some(line_idx) && cursor_line == line_idx,
                    )
                } else {
                    crate::markdown::InlineReveal::None
                };
                let (_, row_char_w, _, _, _) =
                    Self::heading_typography_metrics_with_widths(&classified, &typography, line_char_width, Some(&heading_widths));
                let row_layout = if let Some(tbl) = view.grid_table_at(doc, &tables, line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                    let table_layout = Self::get_or_compute_table_layout(
                        &self.table_layouts,
                        window.text_system(),
                        &|idx| lines_arc.get(idx).map(|s| s.as_str()),
                        tbl,
                        &editor_font,
                        self.manual_table_col_widths.get(&tbl.first_line).map(|w| w.as_slice()),
                        line_char_width,
                        Self::table_room_px(view, line_char_width),
                        &typography,
                    );
                    let cursor_col = if is_act_line && line_idx == cursor_line { Some(active_col_in_row) } else { None };
                    shape_table_row(window.text_system(), line, tbl, line_idx, cursor_col, &table_layout, &typography.headings, &editor_font, row_char_w)
                } else {
                    let raw_row = tables
                        .at(line_idx)
                        .map_or(false, |t| view.table_raw_now(doc, t, cursor_line, self.ctrl_preview_selection || !self.edit_mode));
                    shape_row(&text_system, line, &classified, v, is_act_line || raw_row, &reveal, &typography, &editor_font, row_char_w)
                };

                if line.is_empty() {
                    if sel.start <= line_start && sel.end > line_start {
                        return row_layout
                            .span_rects(0, 1, pixel_scale)
                            .into_iter()
                            .next()
                            .map(|(x, w, _)| (x, w));
                    } else {
                        return None;
                    }
                }

                if let Some(tbl) = view.grid_table_at(doc, &tables, line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                    let sel_s_line = doc.line_of_byte(sel.start);
                    let sel_e_line = doc.line_of_byte(sel.end);
                    if tbl.contains(sel_s_line) && tbl.contains(sel_e_line) {
                        if line_idx == tbl.sep_line() || line_idx < sel_s_line || line_idx > sel_e_line {
                            return None;
                        }
                        let s_line_start = doc.line_start_byte(sel_s_line);
                        let s_char = doc.slice_to_string(s_line_start..sel.start).chars().count();
                        let e_line_start = doc.line_start_byte(sel_e_line);
                        let e_char = doc.slice_to_string(e_line_start..sel.end).chars().count();

                        let s_cells = crate::table::split_row(&lines_arc[sel_s_line]);
                        let e_cells = crate::table::split_row(&lines_arc[sel_e_line]);
                        let col1 = crate::table::cell_index_at(&s_cells, s_char);
                        let col2 = crate::table::cell_index_at(&e_cells, e_char);
                        let col_min = col1.min(col2);
                        let col_max = col1.max(col2);

                        if sel_s_line == sel_e_line {
                            let cur_cells = crate::table::split_row(line);
                            let c_start = cur_cells.get(col_min).map(|c| c.content.start).unwrap_or(0);
                            let c_end = cur_cells.get(col_max).map(|c| c.content.end).unwrap_or(line.chars().count());
                            let row_start = s_char.clamp(c_start, c_end);
                            let row_end = e_char.clamp(c_start, c_end);
                            let (c_s, c_e) = (row_start.min(row_end), row_start.max(row_end));
                            if c_s < c_e {
                                let vis_s = row_layout.column_for_line_char(c_s);
                                let vis_e = row_layout.column_for_line_char(c_e);
                                if vis_s < vis_e {
                                    return row_layout
                                        .span_rects(vis_s, vis_e, pixel_scale)
                                        .into_iter()
                                        .next()
                                        .map(|(x, w, _)| (x, w));
                                }
                            }
                            return None;
                        } else {
                            let table_layout = Self::get_or_compute_table_layout(
                                &self.table_layouts,
                                window.text_system(),
                                &|idx| lines_arc.get(idx).map(|s| s.as_str()),
                                tbl,
                                &editor_font,
                                self.manual_table_col_widths.get(&tbl.first_line).map(|w| w.as_slice()),
                                line_char_width,
                                Self::table_room_px(view, line_char_width),
                                &typography,
                            );
                            let col_x = table_layout.col_x.get(col_min).copied().unwrap_or(0.0);
                            let col_w = if let Some(&max_x) = table_layout.col_x.get(col_max) {
                                let max_w = table_layout.col_widths.get(col_max).copied().unwrap_or(0.0);
                                (max_x + max_w - col_x).max(0.0)
                            } else {
                                table_layout.total_width - col_x
                            };
                            return Some(crate::snap::snapped_span(col_x, col_x + col_w, pixel_scale));
                        }
                    }
                }

                let mut byte_s = sel.start.saturating_sub(line_start).min(line.len());
                let mut byte_e = sel.end.saturating_sub(line_start).min(line.len());
                while byte_s > 0 && !line.is_char_boundary(byte_s) {
                    byte_s -= 1;
                }
                while byte_e > 0 && !line.is_char_boundary(byte_e) {
                    byte_e -= 1;
                }
                let char_s = line[..byte_s].chars().count();
                let char_e = char_s + line[byte_s..byte_e].chars().count();

                let includes_newline = v.is_last_sub && sel.end > line_end && sel.start <= line_end;

                if byte_s < byte_e {
                    let intersect_s = char_s.max(v.char_start);
                    let intersect_e = char_e.min(v.char_end);
                    if intersect_s <= intersect_e {
                        let vis_s = row_layout.column_for_line_char(intersect_s);
                        let mut vis_e = row_layout.column_for_line_char(intersect_e);
                        if includes_newline {
                            vis_e += 1;
                        }
                        if vis_s < vis_e {
                            row_layout
                                .span_rects(vis_s, vis_e, pixel_scale)
                                .into_iter()
                                .next()
                                .map(|(x, w, _)| (x, w))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else if includes_newline && char_s == v.char_end {
                    let vis_s = row_layout.column_for_line_char(v.char_end);
                    row_layout
                                    .span_rects(vis_s, vis_s + 1, pixel_scale)
                                    .into_iter()
                                    .next()
                                    .map(|(x, w, _)| (x, w))
                } else {
                    None
                }
            };

            // Highlighting is only valid while the match list belongs to the
            // document version currently on screen. The list may still answer
            // an older query: it stays visible until the newer scan replies,
            // which keeps highlights from blinking on every keystroke. The
            // per-frame budget keeps a pathological query (one letter in a huge
            // wrapped line) from building tens of thousands of rectangles.
            let find_highlight = is_leaf_active
                && override_view.is_none()
                && self.find_bar.is_open
                && !self.find_bar.matches.is_empty()
                && self.find_bar.matches_version == doc.version;
            let mut find_highlight_budget = 2000usize;

            for v_idx in visible_range {
                let v = &visual_rows_arc[v_idx];
                let line = &lines_arc[v.line_idx];
                let line_start = slice_line_starts[v.line_idx - slice_first_line];
                let line_end = line_start + line.len();
                let is_line_in_selection = selection_range.as_ref().map_or(false, |sel| {
                    if sel.start == sel.end {
                        false
                    } else if line_start == line_end {
                        sel.start <= line_start && sel.end > line_start
                    } else {
                        sel.end > line_start && sel.start < line_end
                    }
                });
                // A selected picture owns the caret visual: keeping the text
                // caret blinking next to the picture reads as two active
                // cursors at once.
                let is_current = self.selected_image_line.is_none()
                    && (self.edit_mode || !is_leaf_active)
                    && (v_idx == active_v_idx);
                let show_current_line_highlight = is_current && !(hide_current_line_on_selection && has_selection);
                let is_in_active_block = code_states_arc[v.line_idx]
                    .as_ref()
                    .map(|s| active_block_ids.contains(&s.block_id))
                    .unwrap_or(false);
                let (is_raw_table_line, is_raw_table_first, is_raw_table_last) = if let Some(tbl) = tables.at(v.line_idx) {
                    if self.source_mode || view.table_raw_now(doc, tbl, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                        (
                            true,
                            v.line_idx == tbl.first_line && v.sub_idx == 0,
                            v.line_idx == tbl.last_line && v.is_last_sub,
                        )
                    } else {
                        (false, false, false)
                    }
                } else {
                    (false, false, false)
                };
                let is_active_line = is_editing && (v.line_idx == cursor_line || is_line_in_selection || is_in_active_block || is_raw_table_line);
                let code_state = code_states_arc[v.line_idx].as_ref().map(|s| (s.kind, s.lang.as_deref()));
                let classified = crate::markdown::classify_line(line, code_state);
                let effective_classified = if self.source_mode {
                    match &classified {
                        crate::markdown::LineKind::CodeFence { is_opening, lang } => crate::markdown::LineKind::CodeFence { is_opening: *is_opening, lang: *lang },
                        crate::markdown::LineKind::CodeLine(raw) => crate::markdown::LineKind::CodeLine(raw),
                        _ => crate::markdown::LineKind::Paragraph(line),
                    }
                } else {
                    classified.clone()
                };
                let reveal = if self.source_mode {
                    crate::markdown::InlineReveal::All
                } else if is_editing {
                    crate::markdown::inline_reveal(
                        line,
                        &classified,
                        (cursor_line == v.line_idx).then_some(cursor_col),
                        selection_chars_in_line(line, line_start, selection_range.as_ref()),
                        view.reveal_all_line == Some(v.line_idx) && cursor_line == v.line_idx,
                    )
                } else {
                    crate::markdown::InlineReveal::None
                };
                let is_opening_fence = matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: true, .. });
                let is_closing_fence = matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: false, .. });
                let is_code_line = matches!(classified, crate::markdown::LineKind::CodeLine(_));
                let callout = if self.source_mode {
                    None
                } else {
                    callout_blocks
                        .get(v.line_idx.wrapping_sub(lines_arc.start()))
                        .copied()
                        .flatten()
                };
                // The block is rounded at its two ends only, and a wrapped row is not
                // an end even when its source line is.
                let callout_first = callout.map_or(false, |block| block.first) && v.sub_idx == 0;
                let callout_last = callout.map_or(false, |block| block.last) && v.is_last_sub;
                let row_len = v.char_end.saturating_sub(v.char_start);

                let (h_metrics_row_h, row_char_w, _row_font_size, _cursor_h, _cursor_top) = if self.source_mode {
                    (typography.row_height, line_char_width, typography.base_font_size, 18.0, 3.0)
                } else {
                    Self::heading_typography_metrics_with_widths(&classified, &typography, line_char_width, Some(&heading_widths))
                };
                let grid_tbl = if self.source_mode {
                    None
                } else {
                    view.grid_table_at(doc, &tables, v.line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode)
                };
                let table_layout = grid_tbl.map(|tbl| {
                    Self::get_or_compute_table_layout(
                        &self.table_layouts,
                        window.text_system(),
                        &|idx| lines_arc.get(idx).map(|s| s.as_str()),
                        tbl,
                        &editor_font,
                        self.manual_table_col_widths.get(&tbl.first_line).map(|w| w.as_slice()),
                        line_char_width,
                        Self::table_room_px(view, line_char_width),
                        &typography,
                    )
                });
                let is_tbl_sep = grid_tbl.map_or(false, |tbl| v.line_idx == tbl.sep_line());
                // Table row heights come from the row geometry itself: a row
                // is as tall as the wrapped lines of its tallest cell, and the
                // height index measured exactly that.
                // A line holding nothing but an image reference paints the
                // picture instead of the markup while the caret is elsewhere.
                // Its height comes from the height index, which measured the
                // scaled picture when the window was built.
                let reference = crate::images::parse_image_reference(line);
                let show_original = reference
                    .as_ref()
                    .is_some_and(|reference| view.shows_original_image(v.line_idx, &reference.target));
                let editing_this = reference
                    .as_ref()
                    .is_some_and(|reference| self.image_editor_matches(v.line_idx, &reference.target));
                let (image_block, broken_image) = if (!self.source_mode
                    && !is_active_line
                    && grid_tbl.is_none()
                    && v.sub_idx == 0
                    && v.is_last_sub)
                    || editing_this
                {
                    match crate::images::resolve_for_note(
                        &doc.path,
                        line,
                        view.window.wrap_cols,
                        show_original,
                    ) {
                        Some(image) => (Some(image), None),
                        // A reference whose file is gone stays a block: it is
                        // shown as broken instead of silently turning into text.
                        None => (None, reference),
                    }
                } else {
                    (None, None)
                };
                let show_image_toolbar = image_block.is_some()
                    && ((self.selected_image_line == Some(v.line_idx)
                        && self.selected_image_doc == Some(doc.id))
                        || self.image_popover.as_ref().map_or(false, |p| p.line == v.line_idx));
                let is_image_reference = crate::images::parse_image_reference(line).is_some();
                let row_h = if grid_tbl.is_some() || image_block.is_some() || is_image_reference {
                    height_index_arc.height_of(v_idx)
                } else {
                    h_metrics_row_h
                };
                // A link painted over a reserved image block uses a text-height
                // band centred in that block. Highlight, selection and caret
                // all share its geometry.
                let image_text_band = is_image_reference && is_active_line;
                let image_band_top = if image_text_band {
                    ((row_h - h_metrics_row_h) / 2.0).max(0.0)
                } else {
                    0.0
                };
                let row_layout: Arc<RowShapedLayout> = if let Some(tbl) = grid_tbl {
                    let cursor_col = if is_active_line && v.line_idx == cursor_line { Some(active_col_in_row) } else { None };
                    Arc::new(shape_table_row(window.text_system(), line, tbl, v.line_idx, cursor_col, table_layout.as_ref().unwrap(), &typography.headings, &editor_font, row_char_w))
                } else {
                    Self::get_or_shape_row(&self.row_shape_cache, &text_system, line, &effective_classified, v, is_active_line, &reveal, &typography, &editor_font, row_char_w)
                };
                let row_layout_down = row_layout.clone();
                let row_layout_move = row_layout.clone();
                let v_down = v.clone();
                let reveal_down = reveal.clone();
                let typography_down = typography.clone();
                let editor_font_down = editor_font.clone();
                let edit_mode_down = self.edit_mode;
                let is_active_down = is_active_line;
                let exact_cursor_down = self.config.editor.exact_reveal_cursor;
                let row_layout_preview: Arc<RowShapedLayout> = if self.source_mode {
                    row_layout.clone()
                } else if is_active_line {
                    if let Some(tbl) = grid_tbl {
                        Arc::new(shape_table_row(window.text_system(), line, tbl, v.line_idx, None, table_layout.as_ref().unwrap(), &typography.headings, &editor_font, row_char_w))
                    } else {
                        // The preview variant (markers hidden) is hashed as a fully
                        // hidden reveal — a separate cache entry.
                        Self::get_or_shape_row(&self.row_shape_cache, &text_system, line, &classified, v, false, &crate::markdown::InlineReveal::None, &typography, &editor_font, row_char_w)
                    }
                } else {
                    row_layout.clone()
                };
                let row_layout_preview_down = row_layout_preview.clone();
                let row_layout_preview_move = row_layout_preview.clone();

                let line_number_text = if v.sub_idx == 0 && !is_tbl_sep {
                    format!("{:>3} ", v.line_idx + 1)
                } else if is_tbl_sep {
                    "".to_string()
                } else {
                    "    ".to_string()
                };

                let row_sel_rect = if line.is_empty() {
                    if selection_range.as_ref().map_or(false, |s| s.start <= line_start && s.end > line_start) {
                        Some(row_layout.span_rects(0, 1, pixel_scale))
                    } else {
                        None
                    }
                } else if let Some(sel) = selection_range.as_ref() {
                    if sel.start < sel.end {
                        let table_rect = if let Some(tbl) = grid_tbl {
                            let sel_s_line = doc.line_of_byte(sel.start);
                            let sel_e_line = doc.line_of_byte(sel.end);
                            if tbl.contains(sel_s_line) && tbl.contains(sel_e_line) {
                                if v.line_idx == tbl.sep_line() || v.line_idx < sel_s_line || v.line_idx > sel_e_line {
                                    Some(None)
                                } else {
                                    let s_line_start = doc.line_start_byte(sel_s_line);
                                    let s_char = doc.slice_to_string(s_line_start..sel.start).chars().count();
                                    let e_line_start = doc.line_start_byte(sel_e_line);
                                    let e_char = doc.slice_to_string(e_line_start..sel.end).chars().count();

                                    let s_cells = crate::table::split_row(&lines_arc[sel_s_line]);
                                    let e_cells = crate::table::split_row(&lines_arc[sel_e_line]);
                                    let col1 = crate::table::cell_index_at(&s_cells, s_char);
                                    let col2 = crate::table::cell_index_at(&e_cells, e_char);
                                    let col_min = col1.min(col2);
                                    let col_max = col1.max(col2);

                                    if sel_s_line == sel_e_line {
                                        let cur_cells = crate::table::split_row(line);
                                        let c_start = cur_cells.get(col_min).map(|c| c.content.start).unwrap_or(0);
                                        let c_end = cur_cells.get(col_max).map(|c| c.content.end).unwrap_or(line.chars().count());
                                        let row_start = s_char.clamp(c_start, c_end);
                                        let row_end = e_char.clamp(c_start, c_end);
                                        let (c_s, c_e) = (row_start.min(row_end), row_start.max(row_end));
                                        if c_s < c_e {
                                            let vis_s = row_layout.column_for_line_char(c_s);
                                            let vis_e = row_layout.column_for_line_char(c_e);
                                            if vis_s < vis_e {
                                                let rects = row_layout.span_rects(vis_s, vis_e, pixel_scale);
                                                if rects.is_empty() { Some(None) } else { Some(Some(rects)) }
                                            } else {
                                                Some(None)
                                            }
                                        } else {
                                            Some(None)
                                        }
                                    } else {
                                        // Whole cells: cover every wrapped line of them.
                                        let cur_cells = crate::table::split_row(line);
                                        let c_start = cur_cells.get(col_min).map(|c| c.content.start).unwrap_or(0);
                                        let c_end = cur_cells.get(col_max).map(|c| c.content.end).unwrap_or(line.chars().count());
                                        let vis_s = row_layout.column_for_line_char(c_start);
                                        let vis_e = row_layout.column_for_line_char(c_end);
                                        let rects = row_layout.span_rects(vis_s, vis_e, pixel_scale);
                                        if rects.is_empty() { Some(None) } else { Some(Some(rects)) }
                                    }
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(rect_opt) = table_rect {
                            rect_opt
                        } else {
                            let mut byte_s = sel.start.saturating_sub(line_start).min(line.len());
                            let mut byte_e = sel.end.saturating_sub(line_start).min(line.len());
                            while byte_s > 0 && !line.is_char_boundary(byte_s) {
                                byte_s -= 1;
                            }
                            while byte_e > 0 && !line.is_char_boundary(byte_e) {
                                byte_e -= 1;
                            }
                            let char_s = line[..byte_s].chars().count();
                            let char_e = char_s + line[byte_s..byte_e].chars().count();

                            let includes_newline = v.is_last_sub && sel.end > line_end && sel.start <= line_end;

                            if byte_s < byte_e {
                                let intersect_s = char_s.max(v.char_start);
                                let intersect_e = char_e.min(v.char_end);
                                if intersect_s <= intersect_e {
                                    let vis_s = row_layout.column_for_line_char(intersect_s);
                                    let mut vis_e = row_layout.column_for_line_char(intersect_e);
                                    if includes_newline {
                                        vis_e += 1;
                                    }
                                    if vis_s < vis_e {
                                        Some(row_layout.span_rects(vis_s, vis_e, pixel_scale))
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else if includes_newline && char_s == v.char_end {
                                let vis_s = row_layout.column_for_line_char(v.char_end);
                                Some(row_layout.span_rects(vis_s, vis_s + 1, pixel_scale))
                            } else {
                                None
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let chip_cell_h = if grid_tbl.is_some() {
                    typography.table_line_height
                } else {
                    row_h
                };
                let chip_inset = code_chip_inset(chip_cell_h);
                // Top of the table content box inside this row (the table keeps
                // an edge margin above the header and below the last row).
                let tbl_edge_top = grid_tbl.map_or(0.0, |tbl| {
                    if v.line_idx == tbl.first_line {
                        crate::markdown::TABLE_EDGE_MARGIN
                    } else {
                        0.0
                    }
                });
                let mut backdrop_rects = Vec::new();
                let mut grammar_layer: Option<gpui::AnyElement> = None;
                let mut whitespace_layer: Option<gpui::AnyElement> = None;
                let mut tag_overlays = Vec::new();
                if !self.source_mode {
                    for &(code_start, code_end) in row_layout.code_ranges() {
                        for (chip_x, chip_w, line_top) in row_layout.span_rects(code_start, code_end, pixel_scale) {
                            let chip_top = tbl_edge_top + line_top + chip_inset;
                            let chip_h = (chip_cell_h - 2.0 * chip_inset).max(1.0);
                            backdrop_rects.push(
                                div()
                                    .absolute()
                                    .left(px(chip_x))
                                    .top(px(chip_top))
                                    .h(px(chip_h))
                                    .w(px(chip_w))
                                    .bg(theme.code_bg)
                                    .rounded_sm(),
                            );
                        }
                    }
                    for (tag_idx, (tag_start, tag_end, tag_name)) in row_layout.tag_ranges().iter().enumerate() {
                        let (bg_color, _) = crate::markdown::tag_color_pair(tag_name, theme.is_dark);
                        let tag_h = (chip_cell_h - 2.0 * chip_inset).max(1.0);
                        let pill_pad = 2.0f32;
                        for (chip_no, (chip_x, chip_w, line_top)) in row_layout
                            .span_rects(*tag_start, *tag_end, pixel_scale)
                            .into_iter()
                            .enumerate()
                        {
                            let tag_top = tbl_edge_top + line_top + chip_inset;
                            backdrop_rects.push(
                                div()
                                    .absolute()
                                    .left(px(chip_x - pill_pad))
                                    .top(px(tag_top))
                                    .h(px(tag_h))
                                    .w(px(chip_w + 2.0 * pill_pad))
                                    .bg(bg_color)
                                    .rounded_full(),
                            );
                            if self.ctrl_held || !self.edit_mode {
                                let tag_id = format!("v-tag-ov-{}-{}-{}-{}-{}", lid_tag, v_idx, tag_idx, chip_no, tag_name);
                                let t = tag_name.clone();
                                tag_overlays.push(
                                    div()
                                        .id(gpui::ElementId::Name(tag_id.into()))
                                        .absolute()
                                        .left(px(chip_x - pill_pad))
                                        .top(px(tag_top))
                                        .h(px(tag_h))
                                        .w(px(chip_w + 2.0 * pill_pad))
                                        .cursor(gpui::CursorStyle::PointingHand)
                                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                            if event.modifiers.control || event.modifiers.platform || !this.edit_mode {
                                                cx.stop_propagation();
                                                this.open_tag_search(&t, window, cx);
                                            }
                                        })),
                                );
                            }
                        }
                    }
                }
                // Grammar lines under the text. A wavy line is a native text
                // underline; the other styles have to be drawn by hand, and a
                // native solid underline cannot be lowered to their level. The
                // default colour is the row's own text colour, so an unset colour
                // inherits it. One canvas per row keeps a dotted line from turning
                // into hundreds of elements.
                if !self.source_mode && grid_tbl.is_none() && !row_layout.grammar_ranges().is_empty() {
                    let base_color = match &classified {
                        crate::markdown::LineKind::Heading1(_) => crate::markdown::heading_color_to_rgb(&headings.h1.color_name),
                        crate::markdown::LineKind::Heading2(_) => crate::markdown::heading_color_to_rgb(&headings.h2.color_name),
                        crate::markdown::LineKind::Heading3(_) => crate::markdown::heading_color_to_rgb(&headings.h3.color_name),
                        crate::markdown::LineKind::Heading4(_) => crate::markdown::heading_color_to_rgb(&headings.h4.color_name),
                        crate::markdown::LineKind::Heading5(_) => crate::markdown::heading_color_to_rgb(&headings.h5.color_name),
                        crate::markdown::LineKind::Heading6(_) => crate::markdown::heading_color_to_rgb(&headings.h6.color_name),
                        crate::markdown::LineKind::Quote(_) => match crate::markdown::callout_prefix(line) {
                            Some(c) => crate::markdown::callout_color(c.kind, theme.is_dark),
                            None => theme.text_secondary,
                        },
                        crate::markdown::LineKind::Task { checked: true, .. } => theme.text_muted,
                        _ => theme.text_primary,
                    };
                    let mut quads = Vec::new();
                    for (start, end, style) in row_layout.grammar_ranges().iter().copied() {
                        if style.line == crate::markdown::GrammarLine::None {
                            continue;
                        }
                        let (line_x, line_w) = row_layout.selection_rect(start, end, pixel_scale);
                        let color = style.line_rgba(base_color);
                        quads.extend(
                            grammar_line_segments(
                                line_x,
                                line_w,
                                style.line,
                                f32::from(row_layout.layout.ascent),
                                f32::from(row_layout.layout.descent),
                                row_h,
                                pixel_scale,
                            )
                            .into_iter()
                            .map(|segment| (segment, color)),
                        );
                    }
                    if !quads.is_empty() {
                        grammar_layer = Some(
                            gpui::canvas(
                                |_, _, _| {},
                                move |bounds, _, window, _| {
                                    // The row itself may start between device
                                    // pixels, so the row-local grid is not enough:
                                    // a four-pixel dot must land on whole screen
                                    // pixels or one of its edges renders flatter.
                                    let scale = window.scale_factor();
                                    let snap = |v: gpui::Pixels| {
                                        if scale > 0.0 {
                                            px((f32::from(v) * scale).round() / scale)
                                        } else {
                                            v
                                        }
                                    };
                                    for ((x, y, w, h, radius), color) in quads {
                                        let origin = bounds.origin + gpui::point(px(x), px(y));
                                        let mut quad = gpui::fill(
                                            gpui::Bounds::new(
                                                gpui::point(snap(origin.x), snap(origin.y)),
                                                gpui::size(px(w), px(h)),
                                            ),
                                            color,
                                        );
                                        if radius > 0.0 {
                                            quad = quad.corner_radii(px(radius));
                                        }
                                        window.paint_quad(quad);
                                    }
                                },
                            )
                            .absolute()
                            .left(px(0.0))
                            .top(px(0.0))
                            .w_full()
                            .h_full()
                            .into_any_element(),
                        );
                    }
                }
                let guide_columns = v.line_idx
                    .checked_sub(lines_arc.start())
                    .and_then(|offset| list_guide_columns.get(offset))
                    .map(|columns| columns.as_slice())
                    .unwrap_or(&[]);

                let mut eol_marker: Option<gpui::AnyElement> = None;
                let preview_sel: Option<(f32, f32)> = if self.ctrl_preview_selection {
                    row_sel_rect
                        .as_ref()
                        .and_then(|rects| rects.first().copied())
                        .map(|(x, w, _)| (x, w))
                } else {
                    None
                };
                if self.edit_mode {
                    // A grid table paints its cells below the table's top edge
                    // margin, so the dots have to start at the same offset.
                    let ws_top_offset = if grid_tbl
                        .is_some_and(|tbl| v.line_idx == tbl.first_line)
                    {
                        crate::markdown::TABLE_EDGE_MARGIN
                    } else {
                        0.0
                    };
                    let ws_quads = whitespace_dot_quads(
                        line,
                        &row_layout,
                        row_h,
                        pixel_scale,
                        guide_columns,
                        preview_sel,
                        self.show_invisibles,
                        ws_top_offset,
                    );
                    if !ws_quads.is_empty() {
                        let dot_color = if self.show_invisibles {
                            let mut c = theme.marked_line_red();
                            c.a = if theme.is_dark { 0.70 } else { 0.75 };
                            c
                        } else {
                            theme.whitespace_dot()
                        };
                        whitespace_layer = Some(
                            gpui::canvas(
                                |_, _, _| {},
                                move |bounds, _, window, _| {
                                    let scale = window.scale_factor();
                                    let snap = |v: gpui::Pixels| {
                                        if scale > 0.0 {
                                            px((f32::from(v) * scale).round() / scale)
                                        } else {
                                            v
                                        }
                                    };
                                    for (x, y, w, h, radius) in ws_quads {
                                        let origin = bounds.origin + gpui::point(px(x), px(y));
                                        let quad = gpui::fill(
                                            gpui::Bounds::new(
                                                gpui::point(snap(origin.x), snap(origin.y)),
                                                gpui::size(px(w), px(h)),
                                            ),
                                            dot_color,
                                        )
                                        .corner_radii(px(radius));
                                        window.paint_quad(quad);
                                    }
                                },
                            )
                            .absolute()
                            .left(px(0.0))
                            .top(px(0.0))
                            .w_full()
                            .h_full()
                            .into_any_element(),
                        );
                    }

                    // A table paints its rows as cells, not as text lines, so
                    // there is no line end to mark inside it.
                    if self.show_invisibles && v.is_last_sub && grid_tbl.is_none() {
                        let end_col = if line.is_empty() || v.char_start >= v.char_end {
                            0
                        } else {
                            row_layout.column_for_line_char(v.char_end)
                        };
                        let eol_x = crate::snap::snap_x(
                            row_layout.x_for_column(end_col) + 3.0,
                            pixel_scale,
                        );
                        let hide_eol = if self.ctrl_preview_selection {
                            if let Some((sel_x, sel_w)) = preview_sel {
                                if sel_w > 0.0 {
                                    let sel_right = sel_x + sel_w;
                                    let covered_by_rect = eol_x >= sel_x - 0.5 && eol_x <= sel_right + 0.5;
                                    let covered_by_range = selection_range.as_ref().map_or(false, |s| {
                                        s.start < s.end && s.end >= line_end && s.start <= line_end
                                    });
                                    covered_by_rect || covered_by_range
                                } else {
                                    false
                                }
                            } else if v.line_idx + 1 >= doc_lines {
                                selection_range.as_ref().map_or(false, |s| {
                                    s.start < s.end && s.end >= line_end && s.start <= line_end
                                })
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        if !hide_eol {
                            let eol_symbol = if v.line_idx + 1 < doc_lines {
                                "¶"
                            } else {
                                "¤"
                            };
                            eol_marker = Some(
                                div()
                                    .absolute()
                                    .left(px(eol_x))
                                    .top(px(0.0))
                                    .h(px(row_h))
                                    .flex()
                                    .items_center()
                                    .font_family("Segoe UI")
                                    .text_sm()
                                    .text_color(theme.marked_line_red())
                                    .child(eol_symbol)
                                    .into_any_element(),
                            );
                        }
                    }
                }
                if let Some(rects) = &row_sel_rect {
                    // Selection rectangles live inside `line-content`, which is
                    // already centred in an image row; their top is local.
                    let edge_top = if image_text_band {
                        0.0
                    } else if let Some(tbl) = grid_tbl {
                        if v.line_idx == tbl.first_line {
                            crate::markdown::TABLE_EDGE_MARGIN
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };
                    // A table cell that wrapped paints one selection rectangle
                    // per sub-line; every other row has exactly one.
                    let sel_h = if image_text_band {
                        h_metrics_row_h
                    } else if grid_tbl.is_some() {
                        typography.table_line_height
                    } else {
                        row_h
                    };
                    let sel_above = if v_idx > 0 { calc_v_row_sel_rect(v_idx - 1) } else { None };
                    let sel_below = if v_idx + 1 < visual_rows_arc.len() { calc_v_row_sel_rect(v_idx + 1) } else { None };
                    let is_preview_selection = self.ctrl_preview_selection || !self.edit_mode;
                    let sel_color = if is_preview_selection {
                        theme.preview_selection_bg()
                    } else {
                        theme.selection_bg
                    };

                    for (idx, &(sel_x, sel_w, sel_top)) in rects.iter().enumerate() {
                        let sel_right = sel_x + sel_w;
                        let is_first = idx == 0;
                        let is_last = idx + 1 == rects.len();
                        let round_tl = is_first
                            && sel_above.map_or(true, |(ax, aw)| sel_x < ax - 0.5 || sel_x >= ax + aw - 0.5);
                        let round_tr = is_first
                            && sel_above.map_or(true, |(ax, aw)| sel_right > ax + aw + 0.5 || sel_right <= ax + 0.5);
                        let round_bl = is_last
                            && sel_below.map_or(true, |(bx, bw)| sel_x < bx - 0.5 || sel_x >= bx + bw - 0.5);
                        let round_br = is_last
                            && sel_below.map_or(true, |(bx, bw)| sel_right > bx + bw + 0.5 || sel_right <= bx + 0.5);

                        let mut sel_rect = div()
                            .absolute()
                            .left(px(sel_x))
                            .top(px(edge_top + sel_top))
                            .h(px(sel_h))
                            .w(px(sel_w))
                            .bg(sel_color);

                        if round_tl {
                            sel_rect = sel_rect.rounded_tl_xs();
                        }
                        if round_tr {
                            sel_rect = sel_rect.rounded_tr_xs();
                        }
                        if round_bl {
                            sel_rect = sel_rect.rounded_bl_xs();
                        }
                        if round_br {
                            sel_rect = sel_rect.rounded_br_xs();
                        }

                        backdrop_rects.push(sel_rect);
                    }
                }

                // Find matches sit above the selection so the active match is
                // still distinguishable when the selection is not on it.
                if find_highlight && !line.is_empty() && find_highlight_budget > 0 {
                    let matches = &self.find_bar.matches;
                    let first = matches.partition_point(|m| m.end <= line_start);
                    for (offset, m) in matches[first..].iter().enumerate() {
                        if m.start >= line_end || find_highlight_budget == 0 {
                            break;
                        }
                        let match_s = m.start.max(line_start) - line_start;
                        let match_e = m.end.min(line_end) - line_start;
                        let char_s = line[..match_s].chars().count();
                        let char_e = char_s + line[match_s..match_e].chars().count();
                        let intersect_s = char_s.max(v.char_start);
                        let intersect_e = char_e.min(v.char_end);
                        if intersect_s >= intersect_e {
                            continue;
                        }
                        let vis_s = row_layout.column_for_line_char(intersect_s);
                        let vis_e = row_layout.column_for_line_char(intersect_e);
                        if vis_s >= vis_e {
                            continue;
                        }
                        let match_h = if image_text_band {
                            h_metrics_row_h
                        } else if grid_tbl.is_some() && !is_tbl_sep {
                            typography.table_line_height
                        } else {
                            row_h
                        };
                        let is_active = first + offset == self.find_bar.active;
                        find_highlight_budget -= 1;
                        let rects = row_layout.span_rects(vis_s, vis_e, pixel_scale);
                        for (match_x, match_w, line_top) in rects {
                            backdrop_rects.push(
                                div()
                                    .absolute()
                                    .left(px(match_x))
                                    .top(px(if image_text_band {
                                        line_top
                                    } else {
                                        tbl_edge_top + line_top
                                    }))
                                    .h(px(match_h))
                                    .w(px(match_w))
                                    .bg(if is_active {
                                        theme.find_match_active_bg()
                                    } else {
                                        theme.find_match_bg()
                                    })
                                    .rounded_xs(),
                            );
                        }
                    }
                }

                let v_char_start = v.char_start;
                let v_char_end = v.char_end;
                let v_line_idx = v.line_idx;
                let v_sub_idx = v.sub_idx;

                let copy_payload: Option<(usize, usize)> =
                    if is_opening_fence && !is_in_active_block {
                        code_states_arc[v.line_idx]
                            .as_ref()
                            .map(|st| (st.block_id, v.line_idx))
                    } else {
                        None
                    };

                if is_opening_fence {
                    if let Some(st) = code_states_arc[v.line_idx].as_ref() {
                        if let Some(lang) = &st.lang {
                            if let Some(owner_plugin) = self.state.code_block_processors.get_plugin_for_lang(lang) {
                                if !self.code_block_views.contains_key(&st.block_id) {
                                    let mut body = String::new();
                                    let mut i = v.line_idx + 1;
                                    while let Some(next) = code_states_arc.get(i).and_then(|s| s.as_ref()) {
                                        if next.block_id != st.block_id || next.kind != crate::markdown::CodeBlockKind::InsideCode {
                                            break;
                                        }
                                        body.push_str(&lines_arc[i]);
                                        body.push('\n');
                                        i += 1;
                                    }
                                    let _ = self.state.plugin_host.send_event(
                                        &owner_plugin,
                                        crate::plugins::api::HostEvent::ProcessCodeBlock {
                                            language: lang.clone(),
                                            source: body,
                                            block_id: st.block_id,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }

                let (custom_ui_node, custom_plugin_id, is_first_code_line) = if self.source_mode {
                    (None, None, false)
                } else {
                    match code_states_arc[v.line_idx].as_ref() {
                        Some(st) => {
                            let view_opt = self.code_block_views.get(&st.block_id);
                            let is_first = st.kind == crate::markdown::CodeBlockKind::InsideCode
                                && v.line_idx > 0
                                && code_states_arc.get(v.line_idx - 1).and_then(|s| s.as_ref()).map(|s| s.block_id) != Some(st.block_id);
                            (view_opt.map(|(_, node)| node), view_opt.map(|(id, _)| id.as_str()), is_first)
                        }
                        None => (None, None, false),
                    }
                };

                let row_image_target = crate::images::parse_image_reference(line)
                    .map(|reference| reference.target);
                let mut row_div = div()
                    .id(gpui::ElementId::Name(format!("v-row-{}-{}", lid_tag, v_idx).into()))
                    .relative()
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .when(grid_tbl.is_none(), |d| d.items_center())
                    .when(grid_tbl.is_some(), |d| d.items_start())
                    .h(px(row_h))
                    .py_0()
                    .px_2();

                if show_current_line_highlight {
                    let is_sep = grid_tbl.map_or(false, |tbl| v.line_idx == tbl.sep_line());
                    let hl_top = if image_text_band {
                        image_band_top
                    } else if let Some(tbl) = grid_tbl {
                        if v.line_idx == tbl.first_line { crate::markdown::TABLE_EDGE_MARGIN } else { 0.0 }
                    } else {
                        0.0
                    };
                    let hl_h = if is_sep {
                        1.0
                    } else if image_text_band {
                        h_metrics_row_h
                    } else if let Some(tbl) = grid_tbl {
                        let edge_bottom = if v.line_idx == tbl.last_line {
                            crate::markdown::TABLE_EDGE_MARGIN
                        } else {
                            0.0
                        };
                        (row_h - hl_top - edge_bottom).max(typography.table_line_height)
                    } else {
                        row_h
                    };

                    if is_leaf_active && !sidebar_rename_focused {
                        row_div = row_div.child(
                            div()
                                .absolute()
                                .left(px(highlight_left))
                                .right(px(highlight_right))
                                .top(px(hl_top))
                                .h(px(hl_h))
                                .bg(theme.bg_hover),
                        );
                    } else {
                        let mut faint_bg = theme.bg_hover;
                        faint_bg.a *= 0.45;
                        let mut border_c = theme.text_muted;
                        border_c.a = 0.5;
                        row_div = row_div.child(
                            div()
                                .absolute()
                                .left(px(highlight_left))
                                .right(px(highlight_right))
                                .top(px(hl_top))
                                .h(px(hl_h))
                                .bg(faint_bg)
                                .border_1()
                                .border_dashed()
                                .border_color(border_c),
                        );
                    }
                }

                // A task checkbox is a click target of its own: the marker cells
                // draw it unless the caret or the selection reveals the raw
                // source there, and clicking it flips the mark instead of
                // placing the caret.
                let task_box = if !self.source_mode && is_leaf_active && v.sub_idx == 0 {
                    crate::markdown::task_prefix(line)
                        .filter(|task| !reveal.shows(0, task.chars))
                        .map(|task| (task.indent_chars + task.list_chars(), task.mark.clone()))
                } else {
                    None
                };

                // Document Y of this row's top, so a click inside a wrapped
                // table cell can resolve the sub-line it landed on.
                let row_doc_top = view.above_h + height_index_arc.top_of(v_idx);
                let row_edge_top = grid_tbl.map_or(0.0, |tbl| {
                    if v_line_idx == tbl.first_line {
                        crate::markdown::TABLE_EDGE_MARGIN
                    } else {
                        0.0
                    }
                });
                if is_leaf_active {
                    row_div = row_div
                        .cursor(gpui::CursorStyle::IBeam)
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            if row_image_target
                                .as_deref()
                                .is_some_and(|target| this.image_editor_matches(v_line_idx, target))
                            {
                                cx.stop_propagation();
                                return;
                            }
                            cx.stop_propagation();
                            if this.is_resizing_any() {
                                return;
                            }
                            // Leaving the caption line commits what was typed.
                            if this.caption_editor.is_some() {
                                this.save_caption_editor(cx);
                            }
                            // Clicking text leaves the picture mode entirely:
                            // the frame, its handles and the floating toolbar
                            // must not outlive the selection they belong to.
                            if row_image_target.is_none() {
                                this.selected_image_line = None;
                                this.selected_image_doc = None;
                                this.hovered_image_line = None;
                                this.hovered_image_doc = None;
                            }
                            let ctrl_held = event.modifiers.control || event.modifiers.platform;
                            this.ctrl_preview_selection = ctrl_held;
                            this.is_mouse_selecting = true;
                            let click_x: f32 = event.position.x.into();
                            let click_y: f32 = event.position.y.into();
                            this.mouse_down_pos = Some((click_x, click_y));
                            this.mouse_drag_started = false;

                            let row_local_y = (f64::from(click_y)
                                - f64::from(this.editor_rows_top_y())
                                + this.current_scroll_top_f64()
                                - row_doc_top) as f32
                                - row_edge_top;
                            let col_in_row = if click_x > text_start_x {
                                let layout = if ctrl_held { &row_layout_preview_down } else { &row_layout_down };
                                let column = layout.column_for_xy(click_x - text_start_x, row_local_y);
                                if (ctrl_held || !this.edit_mode) && event.click_count == 1 {
                                    if let Some((_, _, tag_name)) = layout.tag_ranges().iter().find(|(s, e, _)| column >= *s && column < *e) {
                                        this.open_tag_search(tag_name, window, cx);
                                        this.is_mouse_selecting = false;
                                        this.mouse_drag_started = false;
                                        this.mouse_down_pos = None;
                                        return;
                                    }
                                }
                                layout.line_char_for_column(column).saturating_sub(v_char_start).min(row_len)
                            } else {
                                0
                            };
                            let target_char = v_char_start + col_in_row;
                            if let Some((box_start, mark)) = task_box.as_ref() {
                                if event.click_count == 1
                                    && target_char >= *box_start
                                    && target_char < *box_start + 3
                                {
                                    this.toggle_task_mark(v_line_idx, mark.clone(), window, cx);
                                    this.is_mouse_selecting = false;
                                    this.mouse_drag_started = false;
                                    this.mouse_down_pos = None;
                                    return;
                                }
                            }
                            if let Some((view, doc)) = this.state.active_view_and_doc_mut() {
                                let mut target_char = target_char;
                                if exact_cursor_down && edit_mode_down && !this.source_mode && !ctrl_held && event.click_count == 1 {
                                    let line = &view.window.lines[v_line_idx];
                                    let code_state = view.window.code_states.get(v_line_idx).and_then(|s| s.as_ref()).map(|s| (s.kind, s.lang.as_deref()));
                                    let classified = crate::markdown::classify_line(line, code_state);
                                    let new_reveal = crate::markdown::inline_reveal(
                                        line,
                                        &classified,
                                        Some(target_char),
                                        None,
                                        event.modifiers.alt || view.reveal_all_line == Some(v_line_idx),
                                    );
                                    let reveal_changed = new_reveal != reveal_down && !matches!(new_reveal, crate::markdown::InlineReveal::None);
                                    let heading_activated = !is_active_down && v_down.sub_idx == 0 && matches!(
                                        classified,
                                        crate::markdown::LineKind::Heading1(_)
                                            | crate::markdown::LineKind::Heading2(_)
                                            | crate::markdown::LineKind::Heading3(_)
                                            | crate::markdown::LineKind::Heading4(_)
                                            | crate::markdown::LineKind::Heading5(_)
                                            | crate::markdown::LineKind::Heading6(_)
                                    );
                                    if reveal_changed || heading_activated {
                                        let (cursor_line, _) = view.line_col(doc);
                                        let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
                                        if view.grid_table_at(doc, &tables, v_line_idx, cursor_line, false).is_none() {
                                            let rev_layout = shape_row(
                                                window.text_system(),
                                                line,
                                                &classified,
                                                &v_down,
                                                true,
                                                &new_reveal,
                                                &typography_down,
                                                &editor_font_down,
                                                row_char_w,
                                            );
                                            if click_x > text_start_x {
                                                let rev_col = rev_layout.column_for_x(click_x - text_start_x);
                                                let rev_col_in_row = rev_layout.line_char_for_column(rev_col).saturating_sub(v_char_start).min(row_len);
                                                target_char = v_char_start + rev_col_in_row;
                                            }
                                        }
                                    }
                                }
                                let offset = view.offset_for_line_col(doc, v_line_idx, target_char);
                                if event.click_count == 2 {
                                    let (w_start, w_end) = doc.word_range_at(offset);
                                    view.selection_anchor = Some(w_start);
                                    view.cursor_offset = w_end;
                                    this.is_mouse_selecting = false;
                                    this.mouse_drag_started = false;
                                    this.mouse_down_pos = None;
                                } else if event.click_count >= 3 {
                                    let (l_start, l_end) = doc.line_range_at(offset);
                                    view.selection_anchor = Some(l_start);
                                    view.cursor_offset = l_end;
                                    this.is_mouse_selecting = false;
                                    this.mouse_drag_started = false;
                                    this.mouse_down_pos = None;
                                } else if event.modifiers.shift {
                                    if view.selection_anchor.is_none() {
                                        view.selection_anchor = Some(view.cursor_offset);
                                    }
                                    view.cursor_offset = offset;
                                } else {
                                    view.cursor_offset = offset;
                                    view.selection_anchor = Some(offset);
                                }
                                // Rows stop propagation, so this handler — not
                                // `handle_editor_mouse_down` — is the one that sees a click
                                // on a line. Alt+click must be honoured here or it never is.
                                // A plain click on the revealed line keeps it open; a click
                                // on another line closes the markup again.
                                view.reveal_all_line = if event.modifiers.alt {
                                    Some(v_line_idx)
                                } else if view.reveal_all_line == Some(v_line_idx) {
                                    view.reveal_all_line
                                } else {
                                    None
                                };
                            }
                            window.focus(&this.focus_handle);
                            this.reset_editor_cursor_blink(cx);
                            this.scroll_cursor_into_view();
                            this.state.publish_snapshot();
                            cx.notify();
                        }))
                        .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                            let ctrl_held = event.modifiers.control || event.modifiers.platform;
                            if this.ctrl_held != ctrl_held {
                                this.ctrl_held = ctrl_held;
                                cx.notify();
                            }
                            if this.is_mouse_selecting {
                                let click_x: f32 = event.position.x.into();
                                let click_y: f32 = event.position.y.into();
                                if !this.mouse_drag_started {
                                    if let Some((down_x, down_y)) = this.mouse_down_pos {
                                        let dx = click_x - down_x;
                                        let dy = click_y - down_y;
                                        if dx * dx + dy * dy >= 16.0 {
                                            this.mouse_drag_started = true;
                                        } else {
                                            return;
                                        }
                                    } else {
                                        this.mouse_drag_started = true;
                                    }
                                }
                                cx.stop_propagation();
                                let click_x: f32 = event.position.x.into();
                                let click_y: f32 = event.position.y.into();
                                let row_local_y = (f64::from(click_y)
                                    - f64::from(this.editor_rows_top_y())
                                    + this.current_scroll_top_f64()
                                    - row_doc_top) as f32
                                    - row_edge_top;
                                let col_in_row = if click_x > text_start_x {
                                    let layout = if this.ctrl_preview_selection { &row_layout_preview_move } else { &row_layout_move };
                                    let column = layout.column_for_xy(click_x - text_start_x, row_local_y);
                                    layout.line_char_for_column(column).saturating_sub(v_char_start).min(row_len)
                                } else {
                                    0
                                };
                                let target_char = v_char_start + col_in_row;
                                if let Some((view, doc)) = this.state.active_view_and_doc_mut() {
                                    let new_offset = view.offset_for_line_col(doc, v_line_idx, target_char);
                                    if view.selection_anchor.is_none() {
                                        view.selection_anchor = Some(view.cursor_offset);
                                    }
                                    if view.cursor_offset != new_offset {
                                        view.cursor_offset = new_offset;
                                        view.hold_drag_revealed_tables(doc, this.ctrl_preview_selection || !this.edit_mode);
                                        view.hold_drag_revealed_images(doc, this.ctrl_preview_selection || !this.edit_mode);
                                        this.state.publish_snapshot();
                                        cx.notify();
                                    }
                                }
                            }
                        }));
                }

                let is_editor_focused = is_leaf_active && self.editor_is_focused;
                let cursor_table = tables.at(cursor_line);

                let raw_btn = if is_editing && !self.source_mode {
                    if let Some(tbl) = cursor_table {
                        if v_line_idx == tbl.first_line {
                            let is_raw = view.table_raw_now(doc, tbl, cursor_line, self.ctrl_preview_selection || !self.edit_mode);
                            let (btn_w, btn_h) = (20.0, 18.0);
                            let (btn_left, btn_top) = if is_raw {
                                (row_layout.width() + 18.0, ((row_h - btn_h) / 2.0).max(0.0))
                            } else {
                                let total_w = table_layout.as_ref().map(|l| l.total_width).unwrap_or(0.0);
                                let content_h = (row_h - crate::markdown::TABLE_EDGE_MARGIN)
                                    .max(typography.table_line_height);
                                (total_w + 20.0, crate::markdown::TABLE_EDGE_MARGIN + (content_h - btn_h) / 2.0)
                            };
                            let tbl_first_line = tbl.first_line;
                            let icon_color = if is_raw { theme.accent } else { theme.text_secondary };
                            Some(
                                div()
                                    .id(gpui::ElementId::Name(format!("table-raw-btn-{}", v_idx).into()))
                                    .absolute()
                                    .left(px(btn_left))
                                    .top(px(btn_top))
                                    .w(px(btn_w))
                                    .h(px(btn_h))
                                    .rounded(px(3.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .when(is_raw, |d| {
                                        let mut bg = theme.accent;
                                        bg.a = 0.2;
                                        d.bg(bg).border_1().border_color(theme.accent)
                                    })
                                    .when(!is_raw, |d| {
                                        d.bg(theme.bg_editor).border_1().border_color(theme.border_subtle).hover(|h| h.bg(theme.bg_hover))
                                    })
                                    .child(icons::icon(icons::ICON_CODE, 11.0, icon_color))
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.state.active_toggle_raw_table_at(tbl_first_line);
                                        cx.notify();
                                    }))
                                    .on_mouse_down(MouseButton::Right, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                            )
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let add_col_btn = if is_editing {
                    if let Some(tbl) = grid_tbl {
                        if v_line_idx == tbl.last_line {
                            let total_w = table_layout.as_ref().unwrap().total_width;
                            let current_widths = table_layout.as_ref().unwrap().col_widths.clone();
                            let tbl_clone = tbl.clone();
                            let group_name = format!("tbl-add-col-grp-{}", tbl.first_line);
                            // The strip spans the painted grid, not the row boxes:
                            // the table keeps an edge margin above the header and
                            // below the last row, so the visible grid starts and
                            // ends one margin inside the row heights. The heights
                            // come from the height index the rows are painted at,
                            // so a wrapped or revealed cell cannot shift the strip.
                            let above: f32 = (tbl.first_line..tbl.last_line)
                                .map(|l| {
                                    view.window
                                        .first_row_of_line(l)
                                        .map_or(0.0, |idx| height_index_arc.height_of(idx))
                                })
                                .sum();
                            let table_h = above + row_h;
                            let col_btn_top = crate::markdown::TABLE_EDGE_MARGIN - above;
                            let col_btn_h = (table_h - 2.0 * crate::markdown::TABLE_EDGE_MARGIN)
                                .max(typography.table_line_height);
                            Some(
                                div()
                                    .id(gpui::ElementId::Name(format!("table-add-col-{}", v_idx).into()))
                                    .group(group_name.clone())
                                    .absolute()
                                    .left(px(total_w - 1.0))
                                    .top(px(col_btn_top))
                                    .w(px(14.0))
                                    .h(px(col_btn_h))
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .relative()
                                            .w_full()
                                            .h_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .border_t_1()
                                            .border_r_1()
                                            .border_b_1()
                                            .border_color(theme.border_subtle)
                                            .rounded_r_sm()
                                            .opacity(0.0)
                                            .group_hover(group_name.clone(), |h| {
                                                h.opacity(1.0).border_color(theme.border_subtle)
                                            })
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme.text_secondary)
                                                    .child("+")
                                            )
                                    )
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        let lines: Vec<String> = this.state.active_document().map(|d| Self::table_lines_slice(d, &tbl_clone)).unwrap_or_default();
                                        let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                                        let mut model = crate::table::TableModel::parse(&tbl_clone, &lines_refs);
                                        model.insert_col(model.col_count());
                                        let edit = crate::table::rewrite(&tbl_clone, &model, false, &crate::table::char_width);
                                        let widths = this.manual_table_col_widths
                                            .entry(tbl_clone.first_line)
                                            .or_insert_with(|| current_widths.clone());
                                        if widths.len() == tbl_clone.col_count() {
                                            widths.push(120.0);
                                        }
                                        this.table_layouts.borrow_mut().remove(&tbl_clone.first_line);
                                        this.state.active_apply_table_line_edit(&edit, None);
                                        this.schedule_auto_save(cx);
                                        cx.notify();
                                    }))
                                    .on_mouse_down(MouseButton::Right, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                            )
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                // The strip hangs past the table's last row, right over the line
                // below it. While a mouse selection is being dragged from that
                // line upwards the pointer sweeps this zone on the way into the
                // table, so the strip would pop up under the selection: it stays
                // out of the way until the drag ends.
                let add_row_btn = if is_editing && !self.is_mouse_selecting {
                    if let Some(tbl) = grid_tbl {
                        if v_line_idx == tbl.last_line {
                            let total_w = table_layout.as_ref().unwrap().total_width;
                            let tbl_clone = tbl.clone();
                            let group_name = format!("tbl-add-row-grp-{}", tbl.first_line);
                            Some(
                                div()
                                    .id(gpui::ElementId::Name(format!("table-add-row-{}", v_idx).into()))
                                    .group(group_name.clone())
                                    .absolute()
                                    .left_0()
                                    .top(px((row_h - crate::markdown::TABLE_EDGE_MARGIN).max(typography.table_line_height) - 1.0))
                                    .w(px(total_w))
                                    .h(px(13.0))
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .relative()
                                            .w_full()
                                            .h_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .border_l_1()
                                            .border_r_1()
                                            .border_b_1()
                                            .border_color(theme.border_subtle)
                                            .rounded_b_sm()
                                            .opacity(0.0)
                                            .group_hover(group_name.clone(), |h| {
                                                h.opacity(1.0).border_color(theme.border_subtle)
                                            })
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .line_height(px(13.0))
                                                    .text_color(theme.text_secondary)
                                                    .child("+")
                                            )
                                    )
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        let lines: Vec<String> = this.state.active_document().map(|d| Self::table_lines_slice(d, &tbl_clone)).unwrap_or_default();
                                        let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                                        let mut model = crate::table::TableModel::parse(&tbl_clone, &lines_refs);
                                        model.insert_row(model.rows.len());
                                        let edit = crate::table::rewrite(&tbl_clone, &model, false, &crate::table::char_width);
                                        this.table_layouts.borrow_mut().remove(&tbl_clone.first_line);
                                        this.state.active_apply_table_line_edit(&edit, None);
                                        this.schedule_auto_save(cx);
                                        cx.notify();
                                    }))
                                    .on_mouse_down(MouseButton::Right, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                            )
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let guide_columns = v_line_idx
                    .checked_sub(lines_arc.start())
                    .and_then(|offset| list_guide_columns.get(offset))
                    .map(|columns| columns.as_slice())
                    .unwrap_or(&[]);

                let table_render_arg = grid_tbl.map(|tbl| (tbl, table_layout.as_ref().unwrap().as_ref(), if is_active_line && v_line_idx == cursor_line { Some(active_col_in_row) } else { None }, v_line_idx, row_h));

                let is_tbl_sep = grid_tbl.map_or(false, |tbl| v_line_idx == tbl.sep_line());
                let line_num_top = if image_text_band {
                    image_band_top
                } else if let Some(tbl) = grid_tbl {
                    if v_line_idx == tbl.first_line { crate::markdown::TABLE_EDGE_MARGIN } else { 0.0 }
                } else {
                    0.0
                };
                let line_num_h = if image_text_band {
                    h_metrics_row_h
                } else if is_tbl_sep {
                    0.0
                } else if let Some(tbl) = grid_tbl {
                    let edge_bottom = if v_line_idx == tbl.last_line {
                        crate::markdown::TABLE_EDGE_MARGIN
                    } else {
                        0.0
                    };
                    (row_h - line_num_top - edge_bottom).max(typography.table_line_height)
                } else {
                    row_h
                };

                row_div = row_div
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("line-num-{}-{}", lid_tag, v_idx).into()))
                            .relative()
                            .w(px(line_number_width))
                            .flex_shrink_0()
                            .h(px(row_h))
                            .when(!is_tbl_sep, |d| {
                                d.child(
                                    div()
                                        .absolute()
                                        .right(px(if markers_active { FIND_MARKER_W } else { 0.0 }))
                                        .top(px(line_num_top))
                                        .w(px(line_number_width - if markers_active { FIND_MARKER_W } else { 0.0 }))
                                        .h(px(line_num_h))
                                        .flex()
                                        .items_center()
                                        .justify_end()
                                        .text_xs()
                                        .text_color(if is_current {
                                            if is_leaf_active && !sidebar_rename_focused {
                                                theme.accent
                                            } else {
                                                theme.text_secondary
                                            }
                                        } else {
                                            theme.text_muted
                                        })
                                        .font_family(editor_font.clone())
                                        .child(line_number_text),
                                )
                            })
                            // The marker of a line selected by "Mark all": a red
                            // arrow in the reserved column, left of the text and
                            // right of the number.
                            .when(
                                markers_active
                                    && v.sub_idx == 0
                                    && !is_tbl_sep
                                    && self.find_bar.line_is_marked(doc.id, doc.version, v.line_idx),
                                |d| {
                                    d.child(
                                        div()
                                            .absolute()
                                            .right_0()
                                            .top(px(line_num_top))
                                            .w(px(FIND_MARKER_W))
                                            .h(px(line_num_h))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(icons::icon(
                                                icons::ICON_ARROW_RIGHT,
                                                9.0,
                                                theme.marked_line_red(),
                                            )),
                                    )
                                },
                            )
                    )
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("line-content-{}-{}", lid_tag, v_idx).into()))
                            .relative()
                            .flex()
                            .flex_row()
                            .when(grid_tbl.is_none(), |d| d.items_center())
                            .when(grid_tbl.is_some(), |d| d.items_start())
                            .flex_1()
                            .min_w_0()
                            .whitespace_nowrap()
                            .when(image_text_band, |d| {
                                d.h(px(h_metrics_row_h))
                                    .line_height(px(h_metrics_row_h))
                            })
                            .when(!image_text_band, |d| {
                                d.h(px(row_h)).line_height(px(row_h))
                            })
                            .py_0()
                            .font_family(editor_font.clone())
                            .text_size(px(typography.base_font_size))
                            .cursor(gpui::CursorStyle::IBeam)
                            // The current-line highlight is a child of the row, so a
                            // block wash over the content box would paint on top of
                            // it and swallow it. A highlighted row keeps no wash:
                            // its own highlight is the background above the block.
                            .when(is_opening_fence && !show_current_line_highlight, |s| s.bg(theme.code_bg).rounded_t_md())
                            .when(is_closing_fence && !show_current_line_highlight, |s| s.bg(theme.code_bg).rounded_b_md())
                            .when(is_code_line && !show_current_line_highlight, |s| s.bg(theme.code_bg))
                            // A callout is a block, drawn the way a code block is:
                            // one wash over every row, rounded where the block ends.
                            .when_some(callout, |s, block| {
                                s.bg(crate::markdown::callout_bg(block.kind, theme.is_dark))
                                    .when(callout_first, |s| s.rounded_t_md())
                                    .when(callout_last, |s| s.rounded_b_md())
                            })
                            .when(is_raw_table_line, |s| {
                                let bg = if theme.is_dark { gpui::rgba(0x9333ea24) } else { gpui::rgba(0x7c3aed18) };
                                s.bg(bg)
                            })
                            .when(is_raw_table_first, |s| s.rounded_t_md())
                            .when(is_raw_table_last, |s| s.rounded_b_md())
                            .children(backdrop_rects)
                            .children(grammar_layer)
                            .children(whitespace_layer)
                            .children(eol_marker)
                            .child(
                                // The row content is full-width on purpose. An
                                // auto-width wrapper inside the line box makes a
                                // `w_full()` child resolve against an indefinite
                                // width — a horizontal rule has no text to give the
                                // wrapper its size, so it collapsed to nothing.
                                div()
                                    .w_full()
                                    .when(image_text_band, |d| d.h(px(h_metrics_row_h)))
                                    .when(!image_text_band, |d| d.h_full())
                                    .flex()
                                    .flex_row()
                                    .when(grid_tbl.is_none(), |d| d.items_center())
                                    .when(grid_tbl.is_some(), |d| d.items_start())
                                    .whitespace_nowrap()
                                    .child(match image_block.as_ref() {
                                        Some(image) => {
                                            let editing = self.image_editor.as_ref().filter(|editor| {
                                                editor.line == v_line_idx
                                                    && editor.target == image.reference.target
                                            });
                                            let caption = if editing.is_some() {
                                                None
                                            } else {
                                                image
                                                    .edit
                                                    .caption
                                                    .as_ref()
                                                    .filter(|caption| !caption.text.trim().is_empty())
                                                    .map(|caption| {
                                                        let position = if caption.position.is_empty() {
                                                            self.config.images.caption_position.clone()
                                                        } else {
                                                            caption.position.clone()
                                                        };
                                                        let number = if caption.numbered {
                                                            self.caption_numbers_for(doc)
                                                                .get(&v_line_idx)
                                                                .copied()
                                                        } else {
                                                            None
                                                        };
                                                        (
                                                            crate::images::format_caption(
                                                                &self.config.images.caption_template,
                                                                number,
                                                                &caption.text,
                                                                caption.numbered,
                                                            ),
                                                            position,
                                                        )
                                                    })
                                            };
                                            if let Some(editor) = editing {
                                                let screen_y = f32::from(self.editor_rows_top_y())
                                                    - self.current_scroll_top_f64() as f32
                                                    + row_doc_top as f32
                                                    + crate::images::BLOCK_PADDING
                                                    + crate::images::TOOLBAR_RESERVE;
                                                let mut working = (**image).clone();
                                                working.edit = editor.edit.clone();
                                                working.crop = None;
                                                working.show_original = true;
                                                working.visible = working.natural;
                                                let (w, h) = working.reference.display_size(
                                                    working.natural,
                                                    crate::images::text_width(view.window.wrap_cols),
                                                );
                                                working.width = w;
                                                working.height = h;
                                                // The editor paints the full original, so its
                                                // hit geometry must describe that image, not
                                                // the previously cropped display.
                                                let center_margin =
                                                    ((working.max_width - w) / 2.0).max(0.0);
                                                editor.rect.set((
                                                    text_start_x + center_margin,
                                                    screen_y,
                                                    w,
                                                    h,
                                                ));
                                                editor.scale.set(
                                                    w / working.natural.0.max(1) as f32,
                                                );
                                                self.render_image_block(
                                                    &working,
                                                    v_line_idx,
                                                    false,
                                                    true,
                                                    false,
                                                    None,
                                                    theme,
                                                    window,
                                                    cx,
                                                )
                                            } else {
                                                self.render_image_block(
                                                    image,
                                                    v_line_idx,
                                                    show_image_toolbar && is_leaf_active,
                                                    is_leaf_active,
                                                    self.selected_image_line == Some(v_line_idx)
                                                        && self.selected_image_doc == Some(doc.id),
                                                    caption,
                                                    theme,
                                                    window,
                                                    cx,
                                                )
                                            }
                                        }
                                        None if broken_image.is_some() => {
                                            let target = broken_image
                                                .as_ref()
                                                .map(|reference| reference.target.as_str())
                                                .unwrap_or("");
                                            self.render_broken_image(target, theme)
                                        }
                                        None => render_live_styled_row(
                                        line,
                                        &effective_classified,
                                        v_char_start,
                                        v_char_end,
                                        v_line_idx,
                                        v_sub_idx,
                                        v.is_last_sub,
                                        is_active_line,
                                        &reveal,
                                        self.source_mode || is_in_active_block,
                                        &typography.headings,
                                        theme,
                                        &editor_font,
                                        custom_ui_node,
                                        custom_plugin_id,
                                        is_first_code_line,
                                        self.active_plugin_input.as_ref(),
                                        &self.plugin_input_focus_handle,
                                        self.active_plugin_select.as_ref(),
                                        self.plugin_select_popup.as_ref(),
                                        self.plugin_input_blink.visible(),
                                        table_render_arg,
                                        self.ctrl_held,
                                        self.hovered_link_id.as_deref(),
                                        window,
                                        cx,
                                        line_char_width,
                                        guide_columns,
                                        callout,
                                        self.config.ui.language.as_str(),
                                    )
                                    .into_any_element(),
                                    }),
                            )
                            .children(tag_overlays)
                            .when(raw_btn.is_some(), |s| s.child(raw_btn.unwrap()))
                            .when(add_col_btn.is_some(), |s| s.child(add_col_btn.unwrap()))
                            .when(add_row_btn.is_some(), |s| s.child(add_row_btn.unwrap()))
                            .when(is_current && ((!caret_is_inactive && is_editor_focused && self.editor_blink.visible()) || caret_is_inactive), |s| {
                                let cursor_col = row_layout.column_for_line_char(v_char_start + active_col_in_row);
                                let cursor_x = crate::snap::snap_x(
                                    row_layout.x_for_column(cursor_col),
                                    pixel_scale,
                                );
                                let (caret_w, caret_bg) = if caret_is_inactive {
                                    let mut c = theme.text_muted;
                                    c.a = 0.6;
                                    (1.5, c)
                                } else {
                                    (2.0, theme.accent)
                                };
                                let (caret_top, caret_h) = if image_text_band {
                                    (0.0, h_metrics_row_h)
                                } else if let Some(tbl) = grid_tbl {
                                    // The caret sits on the wrapped sub-line the
                                    // cursor char belongs to, inside the row's
                                    // content box.
                                    let edge = if v_line_idx == tbl.first_line {
                                        crate::markdown::TABLE_EDGE_MARGIN
                                    } else {
                                        0.0
                                    };
                                    let line_top = row_layout.top_for_column(
                                        row_layout.column_for_line_char(v_char_start + active_col_in_row),
                                    );
                                    (edge + line_top, typography.table_line_height)
                                } else {
                                    (0.0, row_h)
                                };
                                s.child(
                                    div()
                                        .absolute()
                                        .left(px(cursor_x))
                                        .top(px(caret_top))
                                        .h(px(caret_h))
                                        .w(px(caret_w))
                                        .bg(caret_bg)
                                        .rounded_xs(),
                                )
                            })
                            .when(copy_payload.is_some(), |s| {
                                let (block_id, fence_line) = copy_payload.unwrap();
                                let just_copied = self.copied_code_block == Some(block_id);
                                s.child(
                                    div()
                                        .id(gpui::ElementId::Name(format!("code-copy-{}-{}", lid_tag, v_idx).into()))
                                        .absolute()
                                        .right(px(6.0))
                                        .top_0()
                                        .h(px(row_h))
                                        .flex()
                                        .items_center()
                                        .px(px(4.0))
                                        .rounded_sm()
                                        .cursor(gpui::CursorStyle::PointingHand)
                                        .hover(|h| h.bg(theme.bg_hover))
                                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                                            cx.stop_propagation();
                                            let body = match this.state.active_view() {
                                                Some(v) => code_block_body(&v.window, fence_line, block_id),
                                                None => return,
                                            };
                                            cx.write_to_clipboard(ClipboardItem::new_string(body));
                                            this.copied_code_block = Some(block_id);
                                            cx.notify();

                                            let entity = cx.weak_entity();
                                            let async_cx = cx.to_async();
                                            let timer_executor = cx.background_executor().clone();
                                            cx.foreground_executor()
                                                .spawn(async move {
                                                    timer_executor
                                                        .timer(std::time::Duration::from_millis(1500))
                                                        .await;
                                                    let _ = async_cx.update(|cx| {
                                                        if let Some(view) = entity.upgrade() {
                                                            view.update(cx, |this, cx| {
                                                                if this.copied_code_block == Some(block_id) {
                                                                    this.copied_code_block = None;
                                                                    cx.notify();
                                                                }
                                                            });
                                                        }
                                                    });
                                                })
                                                .detach();
                                        }))
                                        .child(icons::icon(
                                            if just_copied { icons::ICON_CHECK } else { icons::ICON_COPY },
                                            14.0,
                                            if just_copied { theme.accent } else { theme.text_muted },
                                        )),
                                )
                            }),
                    );

                items.push(row_div);
            }

            let mut editor_list = div()
                .id(gpui::ElementId::from(("note-editor-list", lid_tag)))
                .flex()
                .flex_col()
                .when(show_scrollbar, |d| d.overflow_hidden())
                .size_full()
                .pl(px(NOTE_EDGE_PAD + note_margin_left))
                .pr(px(NOTE_EDGE_PAD + note_margin_right))
                .pt_0();

            if is_leaf_active {
                editor_list = editor_list
                    .track_scroll(&self.editor_scroll_handle)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        if this.is_resizing_any() {
                            return;
                        }
                        cx.stop_propagation();
                        let ctrl_held = event.modifiers.control || event.modifiers.platform;
                        this.ctrl_preview_selection = ctrl_held;
                        this.commit_title_rename(window, cx);
                        window.focus(&this.focus_handle);
                        this.is_mouse_selecting = true;
                        let click_x: f32 = event.position.x.into();
                        let click_y: f32 = event.position.y.into();
                        this.mouse_down_pos = Some((click_x, click_y));
                        this.mouse_drag_started = false;
                        this.handle_editor_mouse_down(click_x, click_y, event.modifiers.shift, event.modifiers.alt, event.click_count, window, cx);
                    }))
                    .on_scroll_wheel(cx.listener(move |this, event: &gpui::ScrollWheelEvent, _, cx| {
                        if this.is_resizing_any() {
                            return;
                        }
                        this.handle_editor_wheel(event, max_scroll_content, cx);
                    }));
            } else {
                editor_list = editor_list
                    .on_scroll_wheel(cx.listener(move |this, event: &gpui::ScrollWheelEvent, _, cx| {
                        if this.is_resizing_any() {
                            return;
                        }
                        if let Some(lid) = leaf_id {
                            if let Some(leaf) = this.state.leaves.get(&lid) {
                                if let Some(tab) = leaf.active_tab() {
                                    if let Some(view) = this.state.views.get_mut(&tab.id) {
                                        let delta = match event.delta {
                                            gpui::ScrollDelta::Lines(lines) => lines.y as f64 * 60.0,
                                            gpui::ScrollDelta::Pixels(pixels) => f32::from(pixels.y) as f64,
                                        };
                                        view.scroll_top = (view.scroll_top - delta).clamp(0.0, max_scroll_content);
                                        if let crate::workspace::TabContent::Document { id } = tab.content {
                                            if let Some(doc) = this.state.documents.get(&id) {
                                                let lines_total = doc.line_index.lines();
                                                view.viewport.offset = view.line_at_doc_y(lines_total, view.scroll_top);
                                            }
                                        }
                                        cx.notify();
                                    }
                                }
                            }
                        }
                    }));
            }

            editor_list = editor_list.child(
                div()
                    .id(gpui::ElementId::from(("note-editor-scroll-content", lid_tag)))
                    .w_full()
                    .flex()
                    .flex_col()
                    .top(px(slice_shift as f32))
                    .children(items),
            );

            editor_container = editor_container.child(editor_list);

            if show_scrollbar {
                let is_resizing = self.is_resizing_any();
                let mut scrollbar_track = div()
                    .id(gpui::ElementId::from(("editor-scrollbar-track", lid_tag)))
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .w(px(track_w))
                    .h_full()
                    .overflow_hidden()
                    .cursor_pointer()
                    .when(!wide_scrollbar && !is_resizing, |d| {
                        d.hover(|h| h.bg(rgba(0x00000015)))
                    })
                    .when(wide_scrollbar, |d| {
                        d.bg(theme_scrollbar.bg_sidebar)
                    });

                if is_leaf_active {
                    scrollbar_track = scrollbar_track
                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                            if this.is_resizing_any() {
                                if this.is_hovering_editor_scrollbar {
                                    this.is_hovering_editor_scrollbar = false;
                                    cx.notify();
                                }
                                return;
                            }
                            if this.is_hovering_editor_scrollbar != *hovered {
                                this.is_hovering_editor_scrollbar = *hovered;
                                cx.notify();
                            }
                        }))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            if this.is_resizing_any() {
                                return;
                            }
                            cx.stop_propagation();
                            let click_y: f32 = event.position.y.into();
                            let click_rel_y = f64::from((click_y - track_top_y).max(0.0));
                            let target_fraction = (click_rel_y / f64::from(viewport_h)).clamp(0.0, 1.0);
                            let target_scroll_y = target_fraction * max_scroll_content;
                            this.set_editor_scroll_top_f64(target_scroll_y);
                            cx.notify();
                        }));
                } else if let Some(lid) = leaf_id {
                    scrollbar_track = scrollbar_track
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                            if this.is_resizing_any() {
                                return;
                            }
                            cx.stop_propagation();
                            this.switch_to_leaf(lid, window, cx);
                        }))
                        .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| {
                            if this.is_resizing_any() {
                                return;
                            }
                            cx.stop_propagation();
                            this.switch_to_leaf(lid, window, cx);
                            this.start_editor_middle_scroll(cx);
                        }));
                }

                let mut scrollbar_thumb = div()
                    .id(gpui::ElementId::from(("editor-scrollbar-thumb", lid_tag)))
                    .absolute()
                    .top(px(thumb_top))
                    .right(px(thumb_right))
                    .w(px(thumb_w))
                    .h(px(thumb_h))
                    .when(wide_scrollbar, |d| d.rounded_none())
                    .when(!wide_scrollbar, |d| d.rounded_full())
                    .bg(thumb_bg)
                    .when(!wide_scrollbar && !is_resizing, |d| {
                        d.hover(|h| h.bg(theme_scrollbar.text_secondary).w(px(8.0)))
                    })
                    .cursor_pointer();

                if is_leaf_active {
                    scrollbar_thumb = scrollbar_thumb
                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                            if this.is_resizing_any() {
                                if this.is_hovering_editor_thumb {
                                    this.is_hovering_editor_thumb = false;
                                    cx.notify();
                                }
                                return;
                            }
                            if this.is_hovering_editor_thumb != *hovered {
                                this.is_hovering_editor_thumb = *hovered;
                                cx.notify();
                            }
                        }))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            if this.is_resizing_any() {
                                return;
                            }
                            cx.stop_propagation();
                            this.is_dragging_scrollbar = true;
                            this.scrollbar_drag_start_y = event.position.y.into();
                            this.scrollbar_drag_start_scroll_y = scroll_y_current;
                            cx.notify();
                        }));
                }

                scrollbar_track = scrollbar_track.child(scrollbar_thumb);

                if let Some((top, h)) = ruler_sel_span {
                    scrollbar_track = scrollbar_track.child(
                        div()
                            .absolute()
                            .top(px(top))
                            .h(px(h))
                            .when(ruler_full, |d| {
                                d.left(px(0.0)).w_full().rounded_xs()
                            })
                            .when(!ruler_full, |d| {
                                d.left(px(ruler_pill_left))
                                    .w(px(ruler_pill_w))
                                    .rounded_full()
                            })
                            .bg(if self.ctrl_preview_selection || !self.edit_mode {
                                theme_scrollbar.preview_selection_bg()
                            } else {
                                theme_scrollbar.selection_bg
                            }),
                    );
                }

                if show_ruler {
                    scrollbar_track = scrollbar_track.child(
                        div()
                            .id(gpui::ElementId::from(("editor-ruler-cursor", lid_tag)))
                            .absolute()
                            .top(px(ruler_cursor_y))
                            .h(px(2.0))
                            .when(ruler_full, |d| {
                                d.left(px(0.0)).w_full().rounded_xs()
                            })
                            .when(!ruler_full, |d| {
                                d.left(px(ruler_pill_left))
                                        .w(px(ruler_pill_w))
                                    .rounded_full()
                            })
                            .bg(if ruler_cursor_under_thumb {
                                theme_scrollbar.border
                            } else {
                                theme_scrollbar.accent
                            }),
                    );
                }

                editor_container = editor_container.child(scrollbar_track);
            }

            // The bands start at the note title, so the navigation bar and the
            // toolbar strips keep their own backgrounds. Painting them last
            // inside this container keeps one continuous column; they have no
            // listeners, so clicks still reach the elements underneath.
            let width_band = |id: &'static str, at_left: bool, width: f32| {
                let band = div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .w(px(width))
                    .bg(theme.width_limit_bg());
                let band = if at_left { band.left_0() } else { band.right_0() };
                band.id(gpui::ElementId::from((id, lid_tag)))
            };

            let mut note_body = div()
                .relative()
                .flex()
                .flex_col()
                .w_full()
                .flex_1()
                .min_w_0()
                .min_h_0();

            note_body = note_body.child(title_box);
            // The find bar belongs directly under the note title panel and
            // above the editor body, so it never floats over the text.
            if is_leaf_active && self.find_bar.is_open {
                note_body = note_body.child(self.render_find_bar(leaf_id, theme, window, cx));
            }
            note_body = note_body.child(editor_container);

            if show_right_band {
                note_body = note_body.child(width_band(
                    "note-width-limit-area",
                    false,
                    NOTE_EDGE_PAD + note_margin_right,
                ));
            }
            if show_left_band {
                note_body = note_body.child(width_band(
                    "note-width-limit-area-left",
                    true,
                    NOTE_EDGE_PAD + note_margin_left,
                ));
            }
            editor_root = editor_root.child(note_body);
        } else if leaf_id
            .and_then(|lid| self.state.leaves.get(&lid))
            .and_then(|l| l.active_tab())
            .map(|t| matches!(t.content, crate::workspace::TabContent::Empty))
            .unwrap_or(is_leaf_active && self.state.active_empty_tab.is_some())
        {
            editor_root = editor_root.child(self.render_new_tab_page(theme, cx));
        } else if is_leaf_active && self.active_center_leaf.is_some() {
            if let Some(leaf_id) = &self.active_center_leaf {
                if let Some(leaf) = self.plugin_leaf_views.get(leaf_id) {
                    editor_root = editor_root.child(
                        div()
                            .id("central-leaf-view")
                            .flex()
                            .flex_col()
                            .size_full()
                            .p(px(16.0))
                            .overflow_scroll()
                            .child(crate::plugins::ui::render_ui_node(
                                &leaf.tree,
                                &leaf.plugin_id,
                                0,
                                theme,
                                self.active_plugin_input.as_ref(),
                                &self.plugin_input_focus_handle,
                                self.active_plugin_select.as_ref(),
                                self.plugin_select_popup.as_ref(),
                                self.plugin_input_blink.visible(),
                                window,
                                cx,
                                None,
                            )),
                    );
                }
            }
        } else {
            editor_root = editor_root.child(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .justify_center()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_3xl()
                            .child("🦅"),
                    )
                    .child(
                        div()
                            .text_xl()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child("Welcome to Vibecodium"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_muted)
                            .child("Select a note from the sidebar or click 'Open Folder' to choose a workspace."),
                    ),
            );
        }

        editor_root
    }
}
