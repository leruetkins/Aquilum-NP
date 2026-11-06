use super::*;

use gpui::AnimationExt;

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        log_fps_tick();
        self.window_width_px = f32::from(window.viewport_size().width);
        self.refresh_monitor_width(window, cx);
        self.ensure_inactive_views_built(window, cx);

        // Advance the optional wheel ease before anything else touches the
        // offset. One-shot writes below (restore/reveal) go through
        // `set_editor_scroll_top_f64`, which cancels the ease, so they win.
        // With the flag off any stray in-flight ease is dropped instead.
        if self.config.editor.wheel_smooth_scroll {
            self.step_wheel_ease(window);
        } else {
            self.wheel_ease.set(None);
        }

        // Device-pixel grid for thin carets: GPUI paints quads at raw float
        // coordinates, so an unsnapped 1.5-2px caret visibly changes thickness
        // while typing. Captured once per frame, reused at every caret site.
        let _pixel_scale = window.scale_factor();

        if !self.initial_scroll_done {
            self.initial_scroll_done = true;
            // Not written yet: until the list is laid out, prepaint clamps
            // against a not-yet-measured max and the restore is lost. The
            // slice below already reads from the pending target, and the
            // write is retried once the bounds are measured.
            self.pending_editor_restore = Some(f64::from(self.initial_scroll_top));
            self.need_reveal_cursor = true;
        }

        // The line at the top of the viewport is what the editor window has to
        // cover, so it is the anchor the window is placed around. Resolved here,
        // once per frame, from the same position the slice below uses — while a
        // restore is pending that is the restore target, not the (not yet
        // written) scroll handle.
        let anchor_doc_y = self
            .pending_editor_restore
            .unwrap_or_else(|| self.current_scroll_top_f64());
        if !self.is_resizing_any() {
            if let Some((view, doc)) = self.state.active_view_and_doc_mut() {
                view.viewport.offset = view.line_at_doc_y(doc.line_index.lines(), anchor_doc_y);
            }
        }

        // Walk the code-fence checkpoints a little further ahead, once per
        // frame. Nothing here is needed for the current frame — it is what
        // makes a later jump to the end of a huge note cheap, without a
        // background thread and without copying the text.
        if let Some(doc) = self.state.active_document_mut() {
            doc.scan_code_ahead(crate::code_scan::SCAN_SLICE_LINES);
        }

        let is_maximized = window.is_maximized();
        let bounds = window.bounds();
        // Live window height for virtualized-list floors (see editor_scroll_metrics,
        // render_sidebar_tree): measured scroll bounds lag one frame behind on
        // creation/maximize, the window size itself is current from frame one.
        let _window_h = f32::from(bounds.size.height);
        let geom = (
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            Some(f32::from(bounds.origin.x)),
            Some(f32::from(bounds.origin.y)),
            is_maximized,
        );
        if self.last_saved_geometry != Some(geom) {
            self.last_saved_geometry = Some(geom);
            let mut ws = crate::state::WindowState::load(&crate::state::app_data_dir());
            ws.maximized = is_maximized;
            if !is_maximized {
                ws.width = geom.0;
                ws.height = geom.1;
                ws.x = geom.2;
                ws.y = geom.3;
            }
            ws.save(&crate::state::app_data_dir());
        }

        let is_custom_hex_focused = self.custom_hex_focus_handle.is_focused(window);
        if !self.hex_is_focused && is_custom_hex_focused {
            self.hex_is_focused = true;
            self.enable_hex_cursor_blink(cx);
        } else if self.hex_is_focused && !is_custom_hex_focused {
            self.hex_is_focused = false;
            self.disable_hex_cursor_blink(cx);
        }

        let is_title_focused = self.title_focus_handle.is_focused(window);
        if !self.was_title_focused && is_title_focused {
            self.enable_title_cursor_blink(cx);
        } else if self.was_title_focused && !is_title_focused {
            self.disable_title_cursor_blink(cx);
            self.commit_title_rename_internal();
        }
        self.was_title_focused = is_title_focused;

        let is_search_focused = self.search_focus_handle.is_focused(window);
        self.quick_filter.sync_focus(is_search_focused, cx);

        // While a find field holds focus the editor is not the focus target,
        // so its caret must not blink next to the field's own caret.
        let is_find_field_focused = self.find_bar.is_open
            && (self.find_bar.query.is_focused(window)
                || self.find_bar.replace.is_focused(window));
        let is_editor_focused_now = self.focus_handle.is_focused(window)
            || (!is_title_focused
                && !is_search_focused
                && !is_custom_hex_focused
                && !is_find_field_focused
                // The inline rename field owns the keyboard, so the editor drops
                // into its unfocused visuals: dashed current-line frame and a
                // static muted caret.
                && !self.bookmark_rename_focus_handle.is_focused(window)
                && !self.folder_modal_focus_handle.is_focused(window)
                && !self.folder_context_menu.is_open
                && self.folder_modal == crate::views::sidebar::FolderModal::None);

        if !self.editor_is_focused && is_editor_focused_now {
            self.editor_is_focused = true;
            self.enable_editor_cursor_blink(cx);
        } else if self.editor_is_focused && !is_editor_focused_now {
            self.editor_is_focused = false;
            self.disable_editor_cursor_blink(cx);
        }

        // Row geometry, the restore and the caret reveal run before `theme`
        // borrows `self`: the reveal may re-place the editor window on the caret,
        // and that needs the note mutably.
        let editor_font = self.editor_font_family();
        let typography = self.editor_typography();
        let line_char_width = self.editor_char_width(cx);

        let viewport_w: f32 = window.viewport_size().width.into();
        let wrap_cols = self.compute_wrap_cols(viewport_w, line_char_width);
        self.last_wrap_cols = wrap_cols;

        // Tables wrap their cells by the painted column budget of each column.
        // Measure them here, before the visual cache is ensured, so the row
        // heights of this very frame already include the wrapped lines.
        let table_cell_cols = match (self.state.active_view(), self.state.active_document()) {
            (Some(view), Some(doc)) => Some(Self::collect_table_cell_cols(
                &self.table_layouts,
                window.text_system(),
                doc,
                view,
                &editor_font,
                &self.manual_table_col_widths,
                line_char_width,
                &typography,
            )),
            _ => None,
        };

        let curr_scroll = self.current_scroll_top_f64();
        let mut new_active_scroll = None;
        if let Some((view, doc)) = self.state.active_view_and_doc_mut() {
            if let Some(cols) = table_cell_cols {
                view.set_table_cell_cols(cols);
            }
            let anchor_line = view.viewport.offset;
            let old_line_top = view.doc_y_of_line(doc.line_index.lines(), anchor_line);
            let px_in_line = (curr_scroll - old_line_top).max(0.0);
            let old_above_h = view.above_h;
            let old_wrap_cols = view.window.wrap_cols;

            view.ensure_visual_cache_with_headings(doc, self.word_wrap, wrap_cols, Some(&typography), anchor_line, self.ctrl_preview_selection || !self.edit_mode);

            // A caret or selection line reflow moves every row below it. When
            // that happens above the anchor line, the anchor's document Y
            // changes and the scroll has to follow it, exactly like a wrap
            // change does.
            let new_line_top = view.doc_y_of_line(doc.line_index.lines(), anchor_line);
            if view.window.wrap_cols != old_wrap_cols
                || (view.above_h - old_above_h).abs() > 0.5
                || (new_line_top - old_line_top).abs() > 0.5
            {
                let first_v_idx = view.window.first_row_of_line(anchor_line).unwrap_or(0);
                let line_local_y = view.window.height_index.top_of(first_v_idx);
                let new_scroll_y = view.above_h + line_local_y + px_in_line;
                new_active_scroll = Some(new_scroll_y);
            }
            if let Some(line) = self.pending_caption_reveal {
                if let Some(v_idx) = view.window.first_row_of_line(line) {
                    let row_top =
                        view.doc_y_of_line(doc.line_index.lines(), line);
                    let row_h = f64::from(view.window.height_index.height_of(v_idx));
                    let viewport_h = f64::from(f32::from(window.viewport_size().height));
                    let row_bottom = row_top + row_h;
                    if row_bottom > curr_scroll + viewport_h - 8.0 {
                        new_active_scroll = Some((row_bottom - viewport_h + 8.0).max(0.0));
                    }
                }
            }
        }
                if self.pending_caption_reveal.is_some() {
            self.pending_caption_reveal = None;
        }
if let Some(new_scroll) = new_active_scroll {
            self.set_editor_scroll_top_f64(new_scroll);
        }
        let active_leaf_id = self.state.workspace.active_leaf;
        let window_h: f32 = window.viewport_size().height.into();
        let (leaf_v_ratio, in_titlebar) = (
            self.state.workspace.vertical_height_ratio_of_leaf(active_leaf_id),
            self.state.workspace.is_leaf_in_titlebar(active_leaf_id),
        );
        let workspace_h = (window_h - 66.0).max(100.0);
        let leaf_total_h = workspace_h * leaf_v_ratio;
        let header_h = if in_titlebar { 90.0 } else { 126.0 };
        let estimated_active_vh = (leaf_total_h - header_h).max(50.0);

        let bounds_h = f32::from(self.editor_scroll_handle.bounds().size.height);
        if bounds_h > 50.0 && (bounds_h - estimated_active_vh).abs() < (estimated_active_vh * 0.35).max(40.0) {
            if let Some(leaf) = self.state.leaves.get(&active_leaf_id) {
                if let Some(tab) = leaf.active_tab() {
                    if let Some(view) = self.state.views.get_mut(&tab.id) {
                        view.measured_viewport_h = Some(bounds_h);
                    }
                }
            }
        }

        // Retry a pending restore until the viewport is measured: the slice below
        // already follows the target, so the rows are right from frame one.
        if self.pending_editor_restore.is_some() {
            let total = self
                .state
                .active_view()
                .map(|v| v.total_height())
                .unwrap_or(0.0);
            let (_, viewport_h, _, _) =
                self.editor_scroll_metrics_for_leaf(Some(active_leaf_id), true, total, window_h);
            if viewport_h > 50.0 {
                if let Some(target) = self.pending_editor_restore.take() {
                    let clamped =
                        Self::clamp_restore_target(target, total, f64::from(viewport_h));
                    if (clamped - self.current_scroll_top_f64()).abs() > 0.5 {
                        self.set_editor_scroll_top_f64(clamped);
                    }
                }
            }
        }
        // The cache above is freshly built, so this reveal is exact — and a
        // no-op when the restored offset already shows the cursor.
        if self.need_reveal_cursor {
            self.need_reveal_cursor = false;
            self.scroll_cursor_into_view();
        }

        self.theme = Theme::from_ui(&self.config.ui, Some(window.appearance()));
        let cx = &*cx;
        let theme = &self.theme;
        let active_path = self.selected_sidebar_folder.clone().or_else(|| self.state.active_document().map(|d| d.path.clone()));
        let _is_editing = self.edit_mode;

        // Draw the frames the scrollbar needs to settle.
        //
        // `editor_scroll_metrics` reads the handle's `bounds` / `max_offset`,
        // which prepaint writes later in this very frame (`clamp_scroll_position`
        // in div.rs), so the numbers used below are always one layout behind. On
        // the first frame after a note is opened they are estimates — a fixed
        // 600 px viewport and the previous note's max — and the thumb is painted
        // from them until some unrelated notify happens to repaint. Ask for the
        // next frame here instead, and keep asking while the inputs still move.
        //
        // This terminates: `inputs` is a pure function of the layout and the
        // window size, and the layout does not depend on it — the scrollbar
        // reserves no width, and the list does not scroll through gpui at all —
        // so two consecutive frames agree and the burst ends. The counter alone
        // is not enough: a resize or a rewrap moves the inputs without going
        // through the arm sites.
        //
        // Arming is thresholded because a text edit also moves `max_scroll` —
        // by a row or two per keystroke. Painting the whole scene one more
        // time for a one-frame-stale thumb on every keystroke is what made
        // key-repeat burn twice the GPU. Any viewport-height change (resize,
        // no explicit hook exists) or a large content jump arms the burst;
        // small deltas do not. `last_scroll_inputs` is always updated so the
        // comparison stays a single-frame delta and never accumulates.
        if let Some(view) = self.state.active_view() {
            let total = view.total_height();
            let (_, viewport_h, max_scroll, _) = self.editor_scroll_metrics(total, window_h);
            let inputs = (viewport_h.round(), max_scroll.round());
            let settling = self.layout_settle_frames.get();
            let prev = self.last_scroll_inputs.get();
            self.last_scroll_inputs.set(inputs);
            let big_jump = (inputs.0 - prev.0).abs() > f32::EPSILON
                || (inputs.1 - prev.1).abs() > f64::from(viewport_h.max(1.0)) * 0.25;
            if settling > 0 || big_jump {
                self.layout_settle_frames.set(settling.saturating_sub(1));
                window.request_animation_frame();
            }
        }

        let is_resizer_active = self.is_resizing_sidebar || self.is_hovering_resizer;
        let resizer_color = if is_resizer_active { theme.accent } else { theme.border };
        div()
            .flex()
            .flex_col()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.bg_app)
            .text_color(theme.text_primary)
            // A click anywhere outside the inline rename field commits it. The
            // field stops its own mouse-down, so this only sees outside clicks.
            // Committing on the press (not the release) keeps a menu action that
            // starts a rename on click from closing the field it just opened.
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.bookmark_rename.is_some() {
                    this.commit_bookmark_rename(cx);
                }
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                let ctrl_held = event.modifiers.control || event.modifiers.platform;
                if this.ctrl_held != ctrl_held {
                    this.ctrl_held = ctrl_held;
                    cx.notify();
                }
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
                } else if let Some(ref drag) = this.split_drag_state.clone() {
                    let current_pos = match drag.direction {
                        crate::workspace::SplitDirection::Row => f32::from(event.position.x),
                        crate::workspace::SplitDirection::Column => f32::from(event.position.y),
                    };
                    let delta = current_pos - drag.start_pos;
                    let ratio_delta = if drag.span > 1.0 { delta / drag.span } else { 0.0 };
                    let new_ratio = drag.initial_ratio + ratio_delta;
                    this.state.workspace.set_split_ratio_at(&drag.path, drag.divider_index, new_ratio);
                    let win_w: f32 = window.bounds().size.width.into();
                    let win_h: f32 = window.bounds().size.height.into();
                    let left_w = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                    let right_w = if this.right_sidebar_collapsed { 0.0 } else { this.right_sidebar_width + 1.0 };
                    this.state.update_leaf_bounds_from_layout(win_w, win_h, left_w, right_w);
                    cx.notify();
                } else if this.sidebar_middle_active {
                    // Middle-button smart scroll keeps working even when the
                    // cursor leaves the tree: the target clamps to the ends.
                    // Takes precedence over scrollbar/selection drags — the
                    // middle button is a separate gesture.
                    let y: f32 = event.position.y.into();
                    this.on_sidebar_smart_mouse_move(y, true, cx);
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
                } else if this.sidebar_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_sidebar_scrollbar_drag(y, cx);
                } else if this.plugin_modal_middle_active {
                    let y: f32 = event.position.y.into();
                    this.on_plugin_modal_middle_mouse_move(y, cx);
                } else if this.plugin_modal_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_plugin_modal_scrollbar_drag(y, cx);
                } else if this.state.active_view().and_then(|v| v.title_input.as_ref()).map(|ti| ti.is_mouse_selecting).unwrap_or(false) {
                    let x: f32 = event.position.x.into();
                    let editor_left_x = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                    let editor_right_x = if this.right_sidebar_collapsed { 0.0 } else { this.right_sidebar_width + 1.0 };
                    let leaf_ratio = this.state.workspace.horizontal_ratio_of_leaf(this.state.workspace.active_leaf);
                    let window_w: f32 = window.bounds().size.width.into();
                    let workspace_w = (window_w - editor_left_x - editor_right_x).max(100.0);
                    let active_lid = Some(this.state.workspace.active_leaf);
                    let title_text_w = this.state.active_view()
                        .and_then(|v| v.title_input.as_ref())
                        .map(|ti| {
                            let run = gpui::TextRun {
                                len: ti.text.len(),
                                font: gpui::font("Segoe UI").bold(),
                                color: Default::default(),
                                underline: Default::default(),
                                strikethrough: None,
                                background_color: None,
                            };
                            let shaped = window.text_system().shape_line(
                                SharedString::from(ti.text.clone()),
                                px(28.0),
                                &[run],
                                None,
                            );
                            f32::from(shaped.width)
                        })
                        .unwrap_or(0.0);
                    let title_left_offset = this.note_title_offset_for_leaf(active_lid, window_w, title_text_w);
                    let title_left_x = editor_left_x + leaf_ratio * workspace_w + title_left_offset;
                    let rel_x = (x - title_left_x).max(0.0);
                    if let Some(view) = this.state.active_view_mut() {
                        if let Some(ref mut input) = view.title_input {
                            let new_pos = if input.text.is_empty() {
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
                            if input.cursor_pos != new_pos {
                                input.cursor_pos = new_pos;
                                this.reset_title_cursor_blink(cx);
                                cx.notify();
                            }
                        }
                    }
                } else if this
                    .caption_editor
                    .as_ref()
                    .is_some_and(|state| state.input.is_mouse_selecting)
                {
                    if let Some(state) = this.caption_editor.as_mut() {
                        state.input.handle_mouse_move(event, window, 12.0, "Segoe UI");
                    }
                    cx.notify();
                } else if this.quick_filter.is_mouse_selecting {
                    this.quick_filter.handle_mouse_move(event, &this.state.search_query, window, cx);
                } else if this
                    .bookmark_rename
                    .as_ref()
                    .is_some_and(|state| state.input.is_mouse_selecting)
                {
                    if let Some(state) = this.bookmark_rename.as_mut() {
                        state.input.handle_mouse_move(event, window, 12.0, "Segoe UI");
                    }
                    cx.notify();
                } else if this.panel_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_panel_scrollbar_drag(y, cx);
                } else if this.recent_files_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_recent_files_scrollbar_drag(y, cx);
                } else if this.sidebar_search.is_mouse_selecting {
                    this.handle_panel_mouse_move(event, window, cx);
                } else if this.catalog_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_catalog_scrollbar_drag(y, cx);
                } else if this.goto_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_goto_scrollbar_drag(y, cx);
                } else if this.search_dialog.is_mouse_selecting {
                    this.handle_goto_mouse_move(event, window, cx);
                } else if this.is_mouse_selecting {
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    let viewport_h: f32 = window.viewport_size().height.into();
                    this.update_mouse_selection(x, y, viewport_h, window, cx);
                } else if this.is_dragging_note_width_slider {
                    let x: f32 = event.position.x.into();
                    let track_x = this.note_width_slider_origin_x.get();
                    let rel_x = x - track_x;
                    const TRACK_W: f32 = 120.0;
                    let new_pct = ((rel_x / TRACK_W) * 100.0).round().clamp(0.0, 100.0) as u32;
                    this.set_editor_width_pct(new_pct, window, cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if let Some(state) = this.caption_editor.as_mut() {
                    if state.input.is_mouse_selecting {
                        state.input.handle_mouse_up();
                        cx.notify();
                    }
                }
                this.quick_filter.handle_mouse_up(cx);
                this.handle_goto_mouse_up(cx);
                this.goto_scrollbar_drag.end();
                this.catalog_scrollbar_drag.end();
                this.handle_panel_mouse_up(cx);
                this.panel_scrollbar_drag.end();
                this.recent_files_scrollbar_drag.end();
                if this.is_resizing_sidebar {
                    // Persist the dragged width right away (crash-safe;
                    // exit save refreshes it again).
                    this.state.session.sidebar_width = this.sidebar_width;
                    this.state.save_session_state();
                }
                if let Some(view) = this.state.active_view_mut() {
                    if let Some(ref mut input) = view.title_input {
                        if input.is_mouse_selecting {
                            input.is_mouse_selecting = false;
                            if input.selection_anchor == Some(input.cursor_pos) {
                                input.selection_anchor = None;
                            }
                            cx.notify();
                        }
                    }
                }
                this.is_resizing_sidebar = false;
                this.is_resizing_right_sidebar = false;
                this.split_drag_state = None;
                this.hovered_split_divider = None;
                this.is_mouse_selecting = false;
                this.mouse_drag_started = false;
                this.mouse_down_pos = None;
                this.is_dragging_scrollbar = false;
                if this.is_dragging_note_width_slider {
                    this.is_dragging_note_width_slider = false;
                    this.save_settings(cx);
                }
                this.sidebar_scrollbar_drag.end();
                this.recent_files_scrollbar_drag.end();
                this.plugin_modal_scrollbar_drag.end();
                if let Some(view) = this.state.active_view_mut() {
                    if !view.drag_revealed_tables.is_empty()
                        || !view.drag_revealed_images.is_empty()
                    {
                        view.drag_revealed_tables.clear();
                        view.drag_revealed_images.clear();
                        view.visual_cache_dirty = true;
                    }
                    if view.selection_anchor == Some(view.cursor_offset) {
                        view.selection_anchor = None;
                    }
                }
                cx.notify();
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if let Some(state) = this.caption_editor.as_mut() {
                    if state.input.is_mouse_selecting {
                        state.input.handle_mouse_up();
                        cx.notify();
                    }
                }
                this.quick_filter.handle_mouse_up(cx);
                this.handle_goto_mouse_up(cx);
                this.goto_scrollbar_drag.end();
                this.handle_panel_mouse_up(cx);
                this.panel_scrollbar_drag.end();
                this.recent_files_scrollbar_drag.end();
                if let Some(view) = this.state.active_view_mut() {
                    if let Some(ref mut input) = view.title_input {
                        if input.is_mouse_selecting {
                            input.is_mouse_selecting = false;
                            if input.selection_anchor == Some(input.cursor_pos) {
                                input.selection_anchor = None;
                            }
                            cx.notify();
                        }
                    }
                }
                this.is_resizing_sidebar = false;
                this.is_resizing_right_sidebar = false;
                this.split_drag_state = None;
                this.hovered_split_divider = None;
                this.is_hovering_resizer = false;
                this.is_hovering_right_resizer = false;
                this.is_mouse_selecting = false;
                this.mouse_drag_started = false;
                this.mouse_down_pos = None;
                this.is_dragging_scrollbar = false;
                if this.is_dragging_note_width_slider {
                    this.is_dragging_note_width_slider = false;
                    this.save_settings(cx);
                }
                this.sidebar_scrollbar_drag.end();
                this.recent_files_scrollbar_drag.end();
                this.plugin_modal_scrollbar_drag.end();
                if let Some(view) = this.state.active_view_mut() {
                    if !view.drag_revealed_tables.is_empty()
                        || !view.drag_revealed_images.is_empty()
                    {
                        view.drag_revealed_tables.clear();
                        view.drag_revealed_images.clear();
                        view.visual_cache_dirty = true;
                    }
                    if view.selection_anchor == Some(view.cursor_offset) {
                        view.selection_anchor = None;
                        this.ctrl_preview_selection = false;
                    }
                }
                cx.notify();
            }))
            .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                // Releasing the middle button anywhere ends smart-scroll
                // gestures (covers releases outside the originating viewport).
                this.stop_sidebar_middle_scroll(cx);
                this.stop_editor_middle_scroll(cx);
                this.stop_plugin_modal_middle_scroll(cx);
            }))
            .on_mouse_up_out(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                this.stop_sidebar_middle_scroll(cx);
                this.stop_editor_middle_scroll(cx);
                this.stop_plugin_modal_middle_scroll(cx);
            }))
            .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _window, cx| {
                let ctrl_held = event.modifiers.control || event.modifiers.platform;
                if this.ctrl_held != ctrl_held {
                    this.ctrl_held = ctrl_held;
                    cx.notify();
                }
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let keystroke = &event.keystroke;
                let key_lower = keystroke.key.to_lowercase();
                if this.handle_toolbar_dropdown_key(&key_lower, window, cx) {
                    return;
                }
                // F2 renames the selected explorer item: the highlighted folder, or
                // the active note when no folder is selected.
                if keystroke.key.eq_ignore_ascii_case("f2")
                    && !keystroke.modifiers.control
                    && !keystroke.modifiers.platform
                    && !keystroke.modifiers.alt
                    && !keystroke.modifiers.shift
                {
                    if this.bookmark_rename.is_none()
                        && !this.folder_context_menu.is_open
                        && !this.file_context_menu.is_open
                    {
                        let selected_folder = this.selected_sidebar_folder.clone().filter(|path| {
                            path.is_dir()
                                && this.state.workspace_path.as_deref() != Some(path.as_path())
                        });
                        let target = selected_folder.map(|path| (path, true)).or_else(|| {
                            this.state
                                .active_document()
                                .map(|doc| (doc.path.clone(), false))
                        });
                        if let Some((path, is_folder)) = target {
                            this.start_sidebar_tree_rename(path, is_folder, window, cx);
                            return;
                        }
                    }
                }
                // Ctrl+P opens Command Palette from anywhere (leaves don't handle it,
                // so this root listener fires exactly once; the modal input
                // stops propagation, so an open modal never re-triggers it).
                if keystroke.modifiers.control
                    && !keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("p")
                        || keystroke.key == "з"
                        || keystroke.key == "З")
                    && !this.command_palette.is_open
                {
                    this.open_command_palette(window, cx);
                }
                // Ctrl+O opens Go to file (Search Dialog) from anywhere.
                if keystroke.modifiers.control
                    && !keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("o")
                        || keystroke.key == "щ"
                        || keystroke.key == "Щ")
                    && !this.search_dialog.is_open
                {
                    this.open_search_dialog(window, cx);
                }
                // Ctrl+Shift+O opens folder dialog from anywhere.
                if keystroke.modifiers.control
                    && keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("o")
                        || keystroke.key == "щ"
                        || keystroke.key == "Щ")
                {
                    this.open_folder_dialog(window, cx);
                }
                // Ctrl+Shift+P also opens Command Palette for muscle memory compatibility.
                if keystroke.modifiers.control
                    && keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("p")
                        || keystroke.key == "з"
                        || keystroke.key == "З")
                {
                    this.open_command_palette(window, cx);
                }
                // Ctrl+Shift+F opens the sidebar search panel from anywhere.
                if keystroke.modifiers.control
                    && keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("f")
                        || keystroke.key == "а"
                        || keystroke.key == "А")
                {
                    this.open_sidebar_search(window, cx);
                }
                // Ctrl+\ splits active leaf to the right
                if keystroke.modifiers.control
                    && !keystroke.modifiers.shift
                    && (keystroke.key == "\\" || keystroke.key == "|")
                {
                    if let Some(doc) = this.state.active_document() {
                        let path = doc.path.clone();
                        this.split_tab_right(&path, window, cx);
                    }
                }
                // Ctrl+Shift+\ splits active leaf down
                if keystroke.modifiers.control
                    && keystroke.modifiers.shift
                    && (keystroke.key == "\\" || keystroke.key == "|")
                {
                    if let Some(doc) = this.state.active_document() {
                        let path = doc.path.clone();
                        this.split_tab_down(&path, window, cx);
                    }
                }
                // Ctrl+W closes active tab
                if keystroke.modifiers.control
                    && !keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("w")
                        || keystroke.key == "ц"
                        || keystroke.key == "Ц")
                {
                    this.close_active_leaf_tab(window, cx);
                }
            }))
            // ================= WINDOW TITLEBAR & TAB BAR (CUSTOM CSD FRAME) =================
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(38.0))
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .bg(theme.bg_sidebar)
                    // Left area: Sidebar header column in titlebar (width matches sidebar)
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .w(px(self.sidebar_width))
                                .flex_shrink_0()
                                .h_full()
                                // The header row's min-content (badge + view buttons) can
                                // exceed a narrow sidebar. Clip it here, like the sidebar
                                // column below, so the overflowing buttons can neither
                                // paint over nor take clicks from the tab strip.
                                .overflow_hidden()
                                .bg(theme.bg_sidebar)
                                .border_b_1()
                                .border_color(theme.border)
                                // 1. Rail header cell (44px) matching the rail below
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(44.0))
                                        .h_full()
                                        .flex_shrink_0()
                                        .border_r_1()
                                        .border_color(theme.border)
                                        .child(
                                            div()
                                                .id("btn-toggle-sidebar")
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .p_1()
                                                .rounded_md()
                                                .cursor_pointer()
                                                .hover(|h| h.bg(theme.bg_hover))
                                                .child(icons::icon(
                                                    icons::ICON_PANEL_LEFT,
                                                    15.0,
                                                    theme.accent,
                                                ))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.toggle_sidebar(cx);
                                                })),
                                        ),
                                )
                                // 2. File tree header (VIBECODIUM badge and drag region)
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .flex_1()
                                        .h_full()
                                        .px_3()
                                        .child(
                                            div()
                                                .px_2()
                                                .py_0p5()
                                                .bg(theme.accent)
                                                .text_color(theme.bg_sidebar)
                                                .font_weight(gpui::FontWeight::BOLD)
                                                .rounded_md()
                                                .text_xs()
                                                // The badge is a free area in the custom titlebar:
                                                // dragging it moves the window, like the empty
                                                // header space to its right.
                                                .window_control_area(WindowControlArea::Drag)
                                                .child("VIBECODIUM"),
                                        )
                                        // Files / Search view toggle (Obsidian-style)
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1()
                                                .mx_1()
                                                .child(
                                                    div()
                                                        .id("btn-sidebar-files")
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .w(px(28.0))
                                                        .h(px(28.0))
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .when(
                                                            self.sidebar_mode
                                                                == crate::views::sidebar::SidebarMode::Files,
                                                            |d| d.bg(theme.bg_hover),
                                                        )
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(
                                                            icons::ICON_FOLDER,
                                                            14.0,
                                                            if self.sidebar_mode == crate::views::sidebar::SidebarMode::Files {
                                                                theme.accent
                                                            } else {
                                                                theme.text_primary
                                                            },
                                                        ))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.close_sidebar_search(cx);
                                                        })),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-sidebar-search")
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .w(px(28.0))
                                                        .h(px(28.0))
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .when(
                                                            self.sidebar_mode
                                                                == crate::views::sidebar::SidebarMode::Search,
                                                            |d| d.bg(theme.bg_hover),
                                                        )
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(
                                                            icons::ICON_SEARCH,
                                                            14.0,
                                                            if self.sidebar_mode == crate::views::sidebar::SidebarMode::Search {
                                                                theme.accent
                                                            } else {
                                                                theme.text_primary
                                                            },
                                                        ))
                                                        .on_click(cx.listener(|this, _, window, cx| {
                                                            this.open_sidebar_search(window, cx);
                                                        })),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-sidebar-bookmarks")
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .w(px(28.0))
                                                        .h(px(28.0))
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .when(
                                                            self.sidebar_mode
                                                                == crate::views::sidebar::SidebarMode::Bookmarks,
                                                            |d| d.bg(theme.bg_hover),
                                                        )
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(
                                                            icons::ICON_BOOKMARK,
                                                            14.0,
                                                            if self.sidebar_mode == crate::views::sidebar::SidebarMode::Bookmarks {
                                                                theme.accent
                                                            } else {
                                                                theme.text_primary
                                                            },
                                                        ))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.open_sidebar_bookmarks(cx);
                                                        })),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-sidebar-recent-files")
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .w(px(28.0))
                                                        .h(px(28.0))
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .when(
                                                            self.sidebar_mode
                                                                == crate::views::sidebar::SidebarMode::RecentFiles,
                                                            |d| d.bg(theme.bg_hover),
                                                        )
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(
                                                            icons::ICON_HISTORY,
                                                            14.0,
                                                            if self.sidebar_mode == crate::views::sidebar::SidebarMode::RecentFiles {
                                                                theme.accent
                                                            } else {
                                                                theme.text_primary
                                                            },
                                                        ))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.open_sidebar_recent_files(cx);
                                                        })),
                                                )
                                                .children(
                                                    self.plugin_leaf_views
                                                        .values()
                                                        .filter(|leaf| leaf.location == "left" && leaf.leaf_id != "recent-files")
                                                        .map(|leaf| {
                                                            let is_active = match &self.sidebar_mode {
                                                                crate::views::sidebar::SidebarMode::Plugin(id) => id == &leaf.leaf_id,
                                                                _ => false,
                                                            };
                                                            let leaf_id = leaf.leaf_id.clone();
                                                            let icon_asset = crate::icons::map_obsidian_icon(leaf.icon.as_deref().unwrap_or("clock"));
                                                            div()
                                                                .id(gpui::ElementId::Name(format!("btn-sidebar-plugin-{}", leaf.leaf_id).into()))
                                                                .flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .w(px(28.0))
                                                                .h(px(28.0))
                                                                .rounded_md()
                                                                .cursor_pointer()
                                                                .when(is_active, |d| d.bg(theme.bg_hover))
                                                                .hover(|h| h.bg(theme.bg_hover))
                                                                .child(icons::icon(
                                                                    icon_asset,
                                                                    14.0,
                                                                    if is_active {
                                                                        theme.accent
                                                                    } else {
                                                                        theme.text_primary
                                                                    },
                                                                ))
                                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                                    this.select_left_leaf(&leaf_id, cx);
                                                                }))
                                                        }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id("sidebar-titlebar-drag-region")
                                                .flex_1()
                                                .h_full()
                                                .window_control_area(WindowControlArea::Drag),
                                        ),
                                ),
                        )
                    })
                    .when(self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(44.0))
                                .h_full()
                                .flex_shrink_0()
                                .border_r_1()
                                .border_b_1()
                                .border_color(theme.border)
                                .bg(theme.bg_sidebar)
                                .child(
                                    div()
                                        .id("btn-toggle-sidebar")
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .p_1()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .hover(|h| h.bg(theme.bg_hover))
                                        .child(icons::icon(
                                            icons::ICON_PANEL_LEFT,
                                            15.0,
                                            theme.text_muted,
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.toggle_sidebar(cx);
                                        })),
                                ),
                        )
                    })
                    // Vertical divider (1px border line + 7px transparent hitbox) matching the sidebar-resizer below exactly
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .id("titlebar-resizer-divider")
                                .relative()
                                .w(px(1.0))
                                .h_full()
                                .flex_shrink_0()
                                .bg(resizer_color)
                                .border_b_1()
                                // Match the vertical line so the accent color reaches the
                                // titlebar's bottom edge instead of stopping one pixel short
                                // under the horizontal border.
                                .border_color(resizer_color)
                                .child(
                                    div()
                                        .id("titlebar-resizer-hitbox")
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .left(px(-3.0))
                                        .w(px(7.0))
                                        .cursor(gpui::CursorStyle::ResizeColumn)
                                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                            this.is_hovering_resizer = *hovered;
                                            cx.notify();
                                        }))
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                            if event.click_count == 2 {
                                                this.toggle_sidebar(cx);
                                            } else {
                                                this.is_resizing_sidebar = true;
                                                cx.notify();
                                            }
                                        })),
                                ),
                        )
                    })
                    // Right area: Editor header column containing Tab Bar, Drag Area, and Window Controls
                    .child(self.render_titlebar_editor_header(is_maximized, theme, window, cx))
                    // Right vertical divider (mirror of `titlebar-resizer-divider`): caps the tab
                    // strip at the right sidebar's left edge and resizes it on drag.
                    .when(!self.right_sidebar_collapsed, |s| {
                        let is_right_resizer_active =
                            self.is_resizing_right_sidebar || self.is_hovering_right_resizer;
                        let right_resizer_color = if is_right_resizer_active {
                            theme.accent
                        } else {
                            theme.border
                        };
                        s.child(
                            div()
                                .id("titlebar-right-resizer-divider")
                                .relative()
                                .w(px(1.0))
                                .h_full()
                                .flex_shrink_0()
                                .bg(right_resizer_color)
                                .border_b_1()
                                .border_color(right_resizer_color)
                                .child(
                                    div()
                                        .id("titlebar-right-resizer-hitbox")
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .right(px(-3.0))
                                        .w(px(7.0))
                                        .cursor(gpui::CursorStyle::ResizeColumn)
                                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                            this.is_hovering_right_resizer = *hovered;
                                            cx.notify();
                                        }))
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                            if event.click_count == 2 {
                                                this.toggle_right_sidebar(cx);
                                            } else {
                                                this.is_resizing_right_sidebar = true;
                                                cx.notify();
                                            }
                                        })),
                                ),
                        )
                    })
                    // Right area: Right sidebar header column in titlebar (width matches sidebar,
                    // mirror of the left sidebar header column). Keeps the window controls glued
                    // to the right window edge while the tab strip ends at the divider.
                    .when(!self.right_sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .id("titlebar-right-header")
                                .flex()
                                .flex_row()
                                .items_center()
                                .w(px(self.right_sidebar_width))
                                .flex_shrink_0()
                                .h_full()
                                .bg(theme.bg_sidebar)
                                .border_b_1()
                                .border_color(theme.border)
                                .child(
                                    div()
                                        .id("titlebar-right-header-drag-region")
                                        .flex_1()
                                        .min_w_0()
                                        .h_full()
                                        .window_control_area(WindowControlArea::Drag),
                                )
                                .child(self.render_window_control_buttons(theme, is_maximized, cx)),
                        )
                    }),
            )
            // ================= MAIN BODY =================
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex_row()
                    .w_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    // ----- Left Action Rail / Ribbon (Obsidian-style) -----
                    .child(self.render_ribbon_rail(theme, cx))
                    // ----- File sidebar (Directory tree) -----
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .flex()
                                .flex_col()
                                .w(px((self.sidebar_width - 44.0).max(100.0)))
                                .flex_shrink_0()
                                .overflow_hidden()
                                .h_full()
                                .bg(theme.bg_sidebar)
                                .pt_2()
                                .px_2()
                                .pb_0()
                                .gap_2()
                                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                                    if this.is_resizing_sidebar {
                                        let x: f32 = event.position.x.into();
                                        if x < 100.0 {
                                            this.sidebar_collapsed = true;
                                        } else {
                                            this.sidebar_collapsed = false;
                                            this.sidebar_width = x.clamp(160.0, 600.0);
                                        }
                                        cx.notify();
                                    }
                                }))
                                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                    if this.is_resizing_sidebar {
                                        this.is_resizing_sidebar = false;
                                        cx.notify();
                                    }
                                }))
                            // Vault Header (Files, Bookmarks, RecentFiles, and Search modes)
                            .when(
                                !matches!(self.sidebar_mode, crate::views::sidebar::SidebarMode::Plugin(_)),
                                |d| {
                                    let (title, actions) = match self.sidebar_mode {
                                        crate::views::sidebar::SidebarMode::Files => {
                                            let is_any_expanded = !self.state.expanded_folders.is_empty();
                                            let expand_icon = if is_any_expanded {
                                                icons::ICON_CHEVRONS_DOWN_UP
                                            } else {
                                                icons::ICON_CHEVRONS_UP_DOWN
                                            };
                                            let actions = div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .id("btn-new-folder")
                                                        .p_1()
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(icons::ICON_FOLDER_PLUS, 14.0, theme.text_muted))
                                                        .on_click(cx.listener(|this, _, window, cx| {
                                                            this.create_new_folder(window, cx);
                                                        })),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-toggle-expand-all")
                                                        .p_1()
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(expand_icon, 14.0, theme.text_muted))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.toggle_expand_all_folders(cx);
                                                        })),
                                                );
                                            (self.t("sidebar.files_title"), Some(actions))
                                        }
                                        crate::views::sidebar::SidebarMode::Bookmarks => {
                                            let is_any_expanded = self.is_any_bookmark_group_expanded();
                                            let expand_icon = if is_any_expanded {
                                                icons::ICON_CHEVRONS_DOWN_UP
                                            } else {
                                                icons::ICON_CHEVRONS_UP_DOWN
                                            };
                                            let actions = div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .id("btn-new-folder")
                                                        .p_1()
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(icons::ICON_FOLDER_PLUS, 14.0, theme.text_muted))
                                                        .on_click(cx.listener(|this, _, window, cx| {
                                                            this.create_bookmark_group(window, cx);
                                                        })),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-toggle-expand-all")
                                                        .p_1()
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(expand_icon, 14.0, theme.text_muted))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.toggle_expand_all_bookmark_groups(cx);
                                                        })),
                                                );
                                            (self.t("sidebar.bookmarks_title"), Some(actions))
                                        }
                                        crate::views::sidebar::SidebarMode::RecentFiles => {
                                            let sort_icon = match self.recent_files_sort_order {
                                                crate::views::sidebar::recent_files::RecentFilesSortOrder::RecentAsc
                                                | crate::views::sidebar::recent_files::RecentFilesSortOrder::AlphabeticalDesc => {
                                                    icons::ICON_SORT_DESC
                                                }
                                                _ => icons::ICON_SORT_ASC,
                                            };
                                            let actions = div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .id("btn-recent-sort")
                                                        .p_1()
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(sort_icon, 14.0, theme.text_muted))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.toggle_recent_files_sort(cx);
                                                        })),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-recent-clear-all")
                                                        .p_1()
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(icons::ICON_TRASH_2, 14.0, theme.text_muted))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.clear_all_recent_files(cx);
                                                        })),
                                                );
                                            (self.t("sidebar.recent_files"), Some(actions))
                                        }
                                        crate::views::sidebar::SidebarMode::Search => {
                                            // Same action row as the other sections:
                                            // its buttons give the header the same height,
                                            // so the input below does not shift when the
                                            // sidebar switches sections.
                                            let actions = div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .id("btn-search-settings")
                                                        .p_1()
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(icons::ICON_SLIDERS, 14.0, theme.text_muted))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.open_settings(None, cx);
                                                        })),
                                                );
                                            (self.t("sidebar.search_title"), Some(actions))
                                        }
                                        _ => (String::new(), None),
                                    };

                                    d.child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_between()
                                            .min_h(px(28.0))
                                            .px_2()
                                            .py_1()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(theme.text_muted)
                                                    .child(title),
                                            )
                                            .children(actions),
                                    )
                                },
                            )
                            // Search Filter Bar + Tree (Files mode) or Bookmarks or Search Panel or Plugin Leaf View
                            .when(
                                self.sidebar_mode
                                    == crate::views::sidebar::SidebarMode::Files,
                                |d| {
                                    d.child(self.render_quick_filter(window, cx))
                                        // Tree Scroll — virtualized list (see views/sidebar/tree.rs)
                                        .child(self.render_sidebar_tree(
                                            active_path.as_ref(),
                                            theme,
                                            window,
                                            cx,
                                        ))
                                },
                            )
                            .when(
                                self.sidebar_mode
                                    == crate::views::sidebar::SidebarMode::Bookmarks,
                                |d| {
                                    d.child(self.render_quick_filter(window, cx))
                                        .child(self.render_sidebar_bookmarks(
                                            active_path.as_ref(),
                                            theme,
                                            window,
                                            cx,
                                        ))
                                },
                            )
                            .when(
                                self.sidebar_mode
                                    == crate::views::sidebar::SidebarMode::RecentFiles,
                                |d| {
                                    d.child(self.render_quick_filter(window, cx))
                                        .child(self.render_sidebar_recent_files(
                                            active_path.as_ref(),
                                            theme,
                                            window,
                                            cx,
                                        ))
                                },
                            )
                            .when(
                                self.sidebar_mode
                                    == crate::views::sidebar::SidebarMode::Search,
                                |d| d.child(self.render_search_panel(theme, window, cx)),
                            )
                            .when(
                                matches!(self.sidebar_mode, crate::views::sidebar::SidebarMode::Plugin(_)),
                                |d| {
                                    if let crate::views::sidebar::SidebarMode::Plugin(ref active_id) = self.sidebar_mode {
                                        if let Some(leaf) = self.plugin_leaf_views.get(active_id) {
                                            d.child(self.render_left_plugin_leaf(leaf, theme, window, cx))
                                        } else {
                                            d
                                        }
                                    } else {
                                        d
                                    }
                                },
                            )
                            // Sidebar Footer (Vault switcher button + Reload button, or Open Folder button if none selected)
                            .child({
                                let has_workspace = self.state.workspace_path.is_some();
                                let mut footer = div()
                                    .id("sidebar-footer")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .px_2()
                                    .py_1p5()
                                    .border_t_1()
                                    .border_color(theme.border_subtle);

                                if has_workspace {
                                    let is_ws_selected = self.selected_sidebar_folder.as_ref().is_some()
                                        && self.selected_sidebar_folder.as_ref() == self.state.workspace_path.as_ref();
                                    footer = footer
                                        .child(
                                            div()
                                                .id("btn-switch-vault")
                                                .flex()
                                                .flex_1()
                                                .min_w_0()
                                                .flex_row()
                                                .items_center()
                                                .gap_2()
                                                .px_1p5()
                                                .py_1()
                                                .rounded_md()
                                                .cursor_pointer()
                                                .when(is_ws_selected, |s| s.bg(theme.bg_selected))
                                                .when(!is_ws_selected, |s| s.hover(|h| h.bg(theme.bg_hover)))
                                                .child(icons::icon(icons::ICON_FOLDER, 15.0, theme.accent))
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .truncate()
                                                        .text_sm()
                                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                                        .text_color(theme.text_primary)
                                                        .child(self.state.workspace_name.clone()),
                                                )
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.open_vault_dialog(cx);
                                                })),
                                        )
                                        .child(
                                            div()
                                                .id("btn-reload-vault")
                                                .p_1()
                                                .rounded_md()
                                                .cursor_pointer()
                                                .hover(|h| h.bg(theme.bg_hover))
                                                .child(icons::icon(icons::ICON_REFRESH, 13.0, theme.text_muted))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.reload_files(window, cx);
                                                })),
                                        );
                                } else {
                                    footer = footer.child(
                                        div()
                                            .id("btn-open-folder")
                                            .flex()
                                            .flex_1()
                                            .flex_row()
                                            .items_center()
                                            .justify_center()
                                            .gap_2()
                                            .px_4()
                                            .py_1p5()
                                            .rounded_lg()
                                            .cursor_pointer()
                                            .bg(theme.accent)
                                            .hover(|h| h.opacity(0.9))
                                            .child(icons::icon(
                                                icons::ICON_FOLDER_OPEN,
                                                15.0,
                                                gpui::rgb(0xffffff),
                                            ))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(gpui::rgb(0xffffff))
                                                    .child(self.t("sidebar.open_folder")),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.open_folder_dialog_deferred(window, cx);
                                            })),
                                    );
                                }

                                footer
                            })
                        )
                    })
                    // ----- Sidebar Resizer Divider (1px border line + 7px transparent hitbox) -----
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .id("sidebar-resizer")
                                .relative()
                                .flex_shrink_0()
                                .w(px(1.0))
                                .h_full()
                                .bg(resizer_color)
                                .child(
                                    div()
                                        .id("sidebar-resizer-hitbox")
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .left(px(-3.0))
                                        .w(px(7.0))
                                        .cursor(gpui::CursorStyle::ResizeColumn)
                                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                            // A scrollbar drag may pass over this hitbox; the
                                            // divider must not light up as the active target.
                                            this.is_hovering_resizer =
                                                *hovered && !this.sidebar_scrollbar_drag.active;
                                            cx.notify();
                                        }))
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                            // One gesture at a time: the scrollbar already
                                            // owns the pointer, so the border must not steal it.
                                            if this.sidebar_scrollbar_drag.active {
                                                return;
                                            }
                                            if event.click_count == 2 {
                                                this.toggle_sidebar(cx);
                                            } else {
                                                this.is_resizing_sidebar = true;
                                                cx.notify();
                                            }
                                        })),
                                ),
                        )
                    })
                    // ----- Central editor area -----
                    .child({
                        let win_w: f32 = window.bounds().size.width.into();
                        let win_h: f32 = window.bounds().size.height.into();
                        let left_w = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
                        let right_w = if self.right_sidebar_collapsed { 0.0 } else { self.right_sidebar_width + 1.0 };
                        let avail_w = (win_w - left_w - right_w).max(100.0);
                        let avail_h = (win_h - 66.0).max(100.0);
                        self.render_workspace_node(&self.state.workspace.root, &[], avail_w, avail_h, theme, window, cx)
                    })
                    // ----- Right Sidebar Resizer & Right Sidebar -----
                    .when(!self.right_sidebar_collapsed, |s| {
                        s.child(self.render_right_resizer(theme, cx))
                            .child(self.render_right_sidebar(theme, window, cx))
                    }),
            )
            // ================= FOOTER / STATUS BAR =================
            .child(
                div()
                    .id("app-status-bar")
                    .flex()
                    .flex_shrink_0()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(28.0))
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .px_4()
                    .bg(theme.bg_sidebar)
                    .border_t_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(self.state.status_text.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .gap_4()
                            .items_center()
                            // Live sync pass sits before the index chip: the
                            // user sees what the toolbar button started, and
                            // the spinner disappears when the pass settles.
                            .when_some(self.sync_status_bar(), |s, (icon, label)| {
                                let icon = icons::icon(icon, 12.0, theme.text_muted).with_animation(
                                    SharedString::from("sync-status-spinner"),
                                    gpui::Animation::new(Duration::from_millis(900)).repeat(),
                                    |icon, delta| {
                                        icon.with_transformation(gpui::Transformation::rotate(
                                            gpui::radians(delta * std::f32::consts::TAU),
                                        ))
                                    },
                                );
                                s.child(
                                    div()
                                        .id("status-sync")
                                        .flex()
                                        .items_center()
                                        .gap_1p5()
                                        .text_color(theme.text_muted)
                                        .child(
                                            div()
                                                .w(px(14.0))
                                                .h(px(14.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(icon),
                                        )
                                        .child(label),
                                )
                            })
                            // Index status chip, ALWAYS visible with a vault
                            // open: "Indexing NN%" while the background build
                            // runs, "Index ready" afterwards — so "is it
                            // done counting?" has a standing answer, not one
                            // that disappears with the build. Click copies
                            // the persisted-index path and prints it in the
                            // status text (a tooltip would need its own view;
                            // this is the lightweight route).
                            .when(self.state.workspace_path.is_some(), |s| {
                                let chip_label =
                                    match self.state.search_worker.build_progress() {
                                        Some(pct) => format!(
                                            "{} {}",
                                            self.t("search.indexing"),
                                            pct
                                        ),
                                        None => self.t("search.index_ready").to_string(),
                                    };
                                s.child(
                                    div()
                                        .id("status-index")
                                        .cursor_pointer()
                                        .hover(|h| h.text_color(theme.text_secondary))
                                        .child(chip_label)
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(path) =
                                                this.state.search_worker.index_file_path()
                                            {
                                                let shown = path.display().to_string();
                                                cx.write_to_clipboard(
                                                    gpui::ClipboardItem::new_string(
                                                        shown.clone(),
                                                    ),
                                                );
                                                let msg = this.t_fmt(
                                                    "search.index_copied",
                                                    &[&shown],
                                                );
                                                this.push_notification(
                                                    crate::views::notification::NotificationType::Info,
                                                    None,
                                                    msg,
                                                    Some(3000),
                                                    cx,
                                                );
                                                cx.notify();
                                            }
                                        })),
                                )
                            })
                            .when_some(self.state.active_document(), |s, doc| {
                                use crate::state::TextStats;
                                let stats_label = match doc.text_stats() {
                                    TextStats::Exact { words, chars } => format!("{}w • {}c", words, chars),
                                    TextStats::Large { bytes } => {
                                        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
                                    }
                                };
                                s.child(
                                    div()
                                        .id("status-text-stats")
                                        .child(stats_label),
                                )
                            })
                            .when_some(self.state.active_view_and_doc(), |s, (view, doc)| {
                                let (line, col) = view.line_col(doc);
                                s.child(format!("Ln {}, Col {}", line + 1, col + 1))
                            })
                            .children(self.state.status_bar.all().into_iter().map(|item| {
                                div()
                                    .id(gpui::SharedString::from(item.id.clone()))
                                    .child(item.text)
                            }))
                            .child(self.render_note_width_slider(theme, cx))
                            .child("120 FPS"),
                    ),
            )
            // ================= TABS POPUP (Obsidian-style switcher) =================
            .when_some(self.tabs_popup.clone(), |d, popup| {
                let window_size = window.bounds().size;
                let window_w: f32 = window_size.width.into();
                let window_h: f32 = window_size.height.into();

                let (left, top) = if let Some((x, y)) = self.tabs_popup_pos {
                    let popup_w = 280.0;
                    let mut l = x - popup_w + 32.0;
                    if l + popup_w > window_w - 8.0 {
                        l = window_w - popup_w - 8.0;
                    }
                    if l < 8.0 {
                        l = 8.0;
                    }
                    let t = (y + 12.0).min(window_h - 100.0).max(8.0);
                    (l, t)
                } else {
                    let popup_right = if self.right_sidebar_collapsed {
                        crate::views::tab_list::POPUP_RIGHT_OFFSET
                    } else {
                        self.right_sidebar_width + 1.0 + 4.0 + 32.0 + 4.0
                    };
                    let l = (window_w - popup_right - 280.0).max(8.0);
                    let t = crate::views::tab_list::WINDOW_ANCHOR_TOP + crate::views::tab_list::POPUP_GAP;
                    (l, t)
                };

                d.child(
                    gpui::deferred(
                        div()
                            .id("tabs-popup-anchor")
                            .absolute()
                            .top(px(top))
                            .left(px(left))
                            .child(popup),
                    )
                    .with_priority(999),
                )
            })
            // ================= COMMAND PALETTE MODAL =================
            .when(self.command_palette.is_open, |d| {
                d.child(self.render_command_palette(theme, window, cx))
            })
            // ================= FOLDER CONTEXT MENU =================
            .when(self.folder_context_menu.is_open, |d| {
                d.child(self.render_folder_context_menu(theme, window, cx))
            })
            // ================= BOOKMARK CONTEXT MENU =================
            .when(self.bookmark_context_menu.is_some(), |d| {
                d.child(
                    gpui::deferred(self.render_bookmark_context_menu(theme, window, cx))
                        .with_priority(1000),
                )
            })
            // ================= FILE CONTEXT MENU =================
            .when(self.file_context_menu.is_open, |d| {
                d.child(self.render_file_context_menu(theme, window, cx))
            })
            // ================= SIDEBAR FILE-TREE DRAG GHOST =================
            .when_some(self.render_sidebar_drag_ghost(theme), |d, ghost| {
                d.child(ghost)
            })
            // ================= BOOKMARKS DRAG GHOST =================
            .when_some(self.render_bookmark_drag_ghost(theme), |d, ghost| {
                d.child(ghost)
            })
            // ================= RIBBON DRAG GHOST =================
            .when_some(self.render_ribbon_drag_ghost(theme), |d, ghost| {
                d.child(ghost)
            })
            // ================= TOOLBAR CONTEXT MENU =================
            .when(self.toolbar_context_menu.is_open, |d| {
                d.child(self.render_toolbar_context_menu(theme, window, cx))
            })
            // ================= TOOLBAR DRAG GHOST =================
            .when_some(self.render_toolbar_drag_ghost(theme), |d, ghost| {
                d.child(ghost)
            })
            // ================= TAB CONTEXT MENU (Obsidian-style) =================
            .when(self.tab_context_menu.is_open, |d| {
                d.child(self.render_tab_context_menu(theme, window, cx))
            })
            // ================= MORE OPTIONS CONTEXT MENU (Obsidian-style 3-dots) =================
            .when(self.more_options_menu.is_open, |d| {
                d.child(self.render_more_options_menu(theme, window, cx))
            })
            // ================= RIBBON CONTEXT MENU (Obsidian-style) =================
            .when(self.ribbon_context_menu.is_open, |d| {
                d.child(self.render_ribbon_context_menu(theme, window, cx))
            })
            // ================= IMAGE SIZE / RADIUS POPOVER =================
            .when(self.image_popover.is_some(), |d| {
                d.child(self.render_image_popover(theme, window, cx))
            })
            // A resize that started on a handle must keep receiving the move
            // and release even when the Size popover is closed.
            .when(
                self.size_drag.is_some()
                    && !self
                        .image_popover
                        .as_ref()
                        .is_some_and(|popover| popover.kind == crate::views::image_ui::ImagePopoverKind::Size),
                |d| d.child(self.render_image_size_drag_capture(cx)),
            )
            // ================= IN-PLACE IMAGE EDITOR =================
            .when(self.image_editor.is_some(), |d| {
                d.child(self.render_image_editor_overlay(theme, window, cx))
            })
            // ================= IMAGE COMPRESSION DIALOG =================
            .when(self.compress_dialog.is_some(), |d| {
                d.child(self.render_compress_dialog(theme, cx))
            })
            // ================= EXPORT MD DIALOG =================
            .when(self.export_dialog.is_some(), |d| {
                d.child(self.render_export_dialog(theme, cx))
            })
            // ================= UNUSED ATTACHMENTS =================
            .when(self.unused_attachments.is_some(), |d| {
                d.child(self.render_unused_attachments(theme, cx))
            })
            // ================= EDITOR CONTEXT MENU =================
            .when(self.editor_context_menu.is_open, |d| {
                d.child(self.render_editor_context_menu(theme, window, cx))
            })
            // ================= TOOLBAR DROPDOWN OVERLAY =================
            .when(self.toolbar_view.open_dropdown.is_some(), |d| {
                if let Some(dd_overlay) = self.render_toolbar_dropdown_overlay(theme, window, cx) {
                    d.child(dd_overlay)
                } else {
                    d
                }
            })
            // ================= TOOLBAR HOVER TOOLTIP =================
            .when(self.toolbar_view.hovered_tooltip.is_some(), |d| {
                if let Some(tip_overlay) = self.render_toolbar_tooltip_overlay(theme, window, cx) {
                    d.child(tip_overlay)
                } else {
                    d
                }
            })
            // ================= GO TO FILE DIALOG =================
            .when(self.search_dialog.is_open, |d| {
                d.child(self.render_search_dialog(theme, window, cx))
            })
            // ================= FOLDER MODALS =================
            .when(self.folder_modal != crate::views::sidebar::FolderModal::None, |d| {
                d.child(self.render_folder_modals(theme, window, cx))
            })
            // ================= VAULT SWITCHER DIALOG =================
            .when(self.vault_dialog_open, |d| {
                d.child(self.render_vault_dialog(theme, window, cx))
            })
            // ================= COMMUNITY PLUGINS CATALOG =================
            .when(self.plugin_catalog.is_open, |d| {
                d.child(self.render_plugin_catalog_modal(window, cx))
            })
            // ================= PLUGIN MODAL DIALOG OVERLAY =================
            .when_some(self.render_plugin_modal_overlay(theme, window, cx), |d, overlay| {
                d.child(overlay)
            })
            // ================= IMAGE FULL-WINDOW VIEWER =================
            .when(self.image_viewer.is_some(), |d| {
                d.child(self.render_image_viewer(theme, window, cx))
            })
            // ================= TOAST NOTIFICATIONS OVERLAY =================
            .when(!self.state.notifications.all().is_empty(), |d| {
                d.child(self.render_notifications(theme, window, cx))
            })
            // ================= MIDDLE-BUTTON SMART SCROLL HINT (editor) =================
            .when(self.editor_middle_active, |d| {
                d.child(
                    gpui::deferred(
                        div()
                            .id("editor-middle-scroll-hint")
                            .absolute()
                            .top(px(48.0))
                            .right(px(16.0))
                            .px(px(10.0))
                            .py(px(4.0))
                            .rounded_full()
                            .bg(theme.bg_hover)
                            .border_1()
                            .border_color(theme.border)
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child("⇅ СКМ-скролл — двигайте мышь, отпустите СКМ"),
                    )
                    .with_priority(1000),
                )
            })
    }
}
