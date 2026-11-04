use super::*;

impl AppView {
    pub(crate) fn handle_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.image_viewer.is_some() {
            self.handle_image_viewer_key(event, _window, cx);
            return;
        }
        if self.image_editor.is_some() {
            self.handle_image_editor_key(event, _window, cx);
            return;
        }
        if self.caption_editor.is_some() {
            self.handle_caption_editor_key(event, _window, cx);
            return;
        }
        if self.compress_dialog.is_some() {
            self.handle_compress_dialog_key(event, _window, cx);
            return;
        }
        if self.export_dialog.is_some() {
            self.handle_export_dialog_key(event, _window, cx);
            return;
        }
        if self.unused_attachments.is_some() {
            self.handle_unused_attachments_key(event, _window, cx);
            return;
        }
        if self.active_plugin_input.is_some() {
            self.handle_plugin_input_key(event, cx);
            return;
        }

        // A selected picture is an object: Delete/Backspace removes the whole
        // reference line instead of editing text under it.
        let delete_key = event.keystroke.key.to_lowercase();
        if (delete_key == "delete" || delete_key == "backspace")
            && !self.editor_context_menu.is_open
            && self.selected_image_doc == self.state.active_doc_id()
        {
            if let Some(line) = self.selected_image_line {
                self.delete_image_at_line(line, cx);
                return;
            }
        }

        // Ctrl+C / Ctrl+X on a selected picture copies or cuts the reference
        // (with attachment metadata), like the context menu entries.
        if (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
            && self.handle_selected_image_clipboard_key(
                &event.keystroke.key,
                true,
                cx,
            )
        {
            return;
        }

        // Esc closes the topmost layer: a popover, then the context menu.
        if event.keystroke.key == "escape" && self.editor_context_menu.is_open {
            self.close_editor_context_menu(cx);
            return;
        }

        // The image size/radius editor owns the keyboard while it is open.
        if self.image_popover.is_some() {
            let key = event.keystroke.key.as_str();
            if key == "escape" {
                self.close_image_popover(_window, cx);
                return;
            }
            let kind = self.image_popover.as_ref().map(|popover| popover.kind);
            if key == "enter" {
                if kind == Some(crate::views::image_ui::ImagePopoverKind::Size) {
                    self.apply_image_size(cx);
                }
                self.close_image_popover(_window, cx);
                return;
            }
            if kind == Some(crate::views::image_ui::ImagePopoverKind::Size) {
                if key == "tab" {
                    if let Some(popover) = self.image_popover.as_mut() {
                        popover.active = match popover.active {
                            crate::views::image_ui::SizeField::Width => crate::views::image_ui::SizeField::Height,
                            crate::views::image_ui::SizeField::Height => crate::views::image_ui::SizeField::Width,
                        };
                        popover.active_input_mut().select_all();
                        let handle = popover.active_input().focus_handle.clone();
                        if let Some(handle) = handle {
                            _window.focus(&handle);
                        }
                    }
                    cx.notify();
                    return;
                }
                if let Some(popover) = self.image_popover.as_mut() {
                    let changed = popover.active_input_mut().handle_key_down(event, cx);
                    let _ = changed;
                }
                return;
            }
            // Radius: arrows nudge the value, everything else is swallowed.
            let step = if event.keystroke.modifiers.shift { 5.0 } else { 1.0 };
            let delta = match key {
                "left" | "down" => Some(-step),
                "right" | "up" => Some(step),
                _ => None,
            };
            if let Some(delta) = delta {
                let current = self
                    .image_popover
                    .as_ref()
                    .map(|popover| crate::images::load_edit(&popover.path).corner_radius.unwrap_or(0.0))
                    .unwrap_or(0.0);
                self.set_image_radius((current + delta).clamp(0.0, 50.0), cx);
            }
            return;
        }

        self.reset_editor_cursor_blink(cx);
        let keystroke = &event.keystroke;
        let ctrl = keystroke.modifiers.control || keystroke.modifiers.platform;
        if ctrl != self.ctrl_held {
            self.ctrl_held = ctrl;
            cx.notify();
        }
        let alt = keystroke.modifiers.alt;
        let shift = keystroke.modifiers.shift;
        if alt && keystroke.key == "left" {
            self.navigate_back(_window, cx);
            return;
        }

        // Alt + Right (Navigate Forward in note history)
        if alt && keystroke.key == "right" {
            self.navigate_forward(_window, cx);
            return;
        }

        // Escape closes plugin modal dialog if open
        if keystroke.key == "escape" && self.has_active_plugin_modal() {
            self.close_active_plugin_modal(cx);
            return;
        }

        // Escape closes the tabs popup if open
        if keystroke.key == "escape" && self.tabs_popup.is_some() {
            self.close_tabs_popup(cx);
            return;
        }

        // Escape closes the command palette if open
        if keystroke.key == "escape" && self.command_palette.is_open {
            self.close_command_palette(_window, cx);
            return;
        }

        // Escape closes the in-editor find bar when focus is back in the editor
        if keystroke.key == "escape" && self.find_bar.is_open {
            self.close_find_bar(_window, cx);
            return;
        }

        // Escape closes folder context menu if open
        if keystroke.key == "escape" && self.folder_context_menu.is_open {
            self.close_folder_context_menu(cx);
            return;
        }

        // Escape closes bookmark context menu if open
        if keystroke.key == "escape" && self.bookmark_context_menu.is_some() {
            self.close_bookmark_context_menu(cx);
            return;
        }

        // Escape cancels bookmark rename if active
        if keystroke.key == "escape" && self.bookmark_rename.is_some() {
            self.cancel_bookmark_rename(cx);
            return;
        }

        // Escape closes file context menu if open
        if keystroke.key == "escape" && self.file_context_menu.is_open {
            self.close_file_context_menu(cx);
            return;
        }

        // Escape closes more options context menu if open
        if keystroke.key == "escape" && self.more_options_menu.is_open {
            self.close_more_options_menu(cx);
            return;
        }

        // Escape closes folder modal if open
        if keystroke.key == "escape" && self.folder_modal != crate::views::sidebar::FolderModal::None {
            self.close_folder_modal(cx);
            return;
        }

        // Escape closes plugin select popup if open
        if keystroke.key == "escape" && self.plugin_select_popup.is_some() {
            self.close_plugin_select(cx);
            return;
        }

        // Escape closes settings window if open
        if keystroke.key == "escape" && self.settings_window_manager.is_open() {
            self.close_settings(cx);
            return;
        }

        // Escape cancels selection if active
        if keystroke.key == "escape" {
            self.ctrl_preview_selection = false;
            if let Some(view) = self.state.active_view_mut() {
                if view.reveal_all_line.is_some() {
                    view.reveal_all_line = None;
                    cx.notify();
                }
                if view.selection_anchor.is_some() && view.selection_anchor != Some(view.cursor_offset) {
                    view.selection_anchor = None;
                    cx.notify();
                    return;
                }
            }
        }

        if !ctrl && keystroke.key != "control" && keystroke.key != "shift" && keystroke.key != "alt" {
            self.ctrl_preview_selection = false;
        }

        // Ctrl + , (Toggle Settings modal)
        if ctrl && (keystroke.key == "," || keystroke.key == "<" || keystroke.key == "б" || keystroke.key == "Б") {
            self.toggle_settings(cx);
            return;
        }

        // Ctrl + \ or Ctrl + B (Collapse / expand the sidebar)
        if ctrl && (keystroke.key == "\\" || keystroke.key.eq_ignore_ascii_case("b") || keystroke.key == "и" || keystroke.key == "И") {
            self.toggle_sidebar(cx);
            return;
        }

        // Ctrl + S (Save)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("s") || keystroke.key == "ы" || keystroke.key == "Ы") {
            self.flush_active_note_now(cx);
            return;
        }

        // Ctrl + P (Command palette)
        if ctrl && !shift && (keystroke.key.eq_ignore_ascii_case("p") || keystroke.key == "з" || keystroke.key == "З") {
            self.open_command_palette(_window, cx);
            return;
        }

        // Ctrl + Shift + P (Command palette)
        if ctrl && shift && (keystroke.key.eq_ignore_ascii_case("p") || keystroke.key == "з" || keystroke.key == "З") {
            self.open_command_palette(_window, cx);
            return;
        }

        // Ctrl + O (Search notes / Quick switcher)
        if ctrl && !shift && (keystroke.key.eq_ignore_ascii_case("o") || keystroke.key == "щ" || keystroke.key == "Щ") {
            self.open_search_dialog(_window, cx);
            return;
        }

        // Ctrl + Shift + O (Open vault)
        if ctrl && shift && (keystroke.key.eq_ignore_ascii_case("o") || keystroke.key == "щ" || keystroke.key == "Щ") {
            self.open_folder_dialog(_window, cx);
            return;
        }

        // Ctrl + N (New empty tab: no file is created, New Tab is shown)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("n") || keystroke.key == "т" || keystroke.key == "Т") {
            self.open_empty_tab(_window, cx);
            return;
        }

        // Ctrl + W (Close current tab)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("w") || keystroke.key == "ц" || keystroke.key == "Ц") {
            if let Some(doc) = self.state.active_document() {
                let path = doc.path.clone();
                self.close_tab(&path, cx);
            }
            return;
        }

        // Ctrl + F (Find and replace in note). Shift excluded:
        // Ctrl+Shift+F opens the sidebar search panel via the root listener.
        if ctrl && !shift && (keystroke.key.eq_ignore_ascii_case("f") || keystroke.key == "а" || keystroke.key == "А") {
            self.open_find_bar(_window, cx);
            return;
        }

        // Ctrl + Z (Undo)
        if ctrl && !shift && (keystroke.key.eq_ignore_ascii_case("z") || keystroke.key == "я" || keystroke.key == "Я") {
            if self.state.active_undo() {
                cx.notify();
            }
            self.schedule_auto_save(cx);
            return;
        }

        // Ctrl + Y or Ctrl + Shift + Z (Redo)
        if (ctrl && (keystroke.key.eq_ignore_ascii_case("y") || keystroke.key == "н" || keystroke.key == "Н"))
            || (ctrl && shift && (keystroke.key.eq_ignore_ascii_case("z") || keystroke.key == "я" || keystroke.key == "Я"))
        {
            if self.state.active_redo() {
                cx.notify();
            }
            self.schedule_auto_save(cx);
            return;
        }

        // Ctrl + A (Select all)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("a") || keystroke.key == "ф" || keystroke.key == "Ф") {
            self.ctrl_preview_selection = false;
            self.state.active_select_all();
            cx.notify();
            return;
        }

        // Ctrl + C (Copy)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("c") || keystroke.key == "с" || keystroke.key == "С") {
            let copied_tsv = if let Some((view, doc)) = self.state.active_view_and_doc() {
                let (cursor_line, _) = view.line_col(doc);
                if let Some(range) = view.selection_range(doc.len()) {
                    let start_line = doc.line_of_byte(range.start);
                    let end_line = doc.line_of_byte(range.end);
                    // The cursor is one end of the selection, so the windowed
                    // scan covers it too; a full-document scan is not needed.
                    let tables = doc.ensure_table_index_for_window(start_line, end_line);
                    if let (Some(t1), Some(t2)) = (tables.at(start_line), tables.at(end_line)) {
                        if t1.first_line == t2.first_line && view.grid_table_at(doc, &tables, cursor_line, cursor_line, self.ctrl_preview_selection || !self.edit_mode).is_some() {
                            let start_line_byte = doc.line_start_byte(start_line);
                            let end_line_byte = doc.line_start_byte(end_line);
                            let start_col = doc.slice_to_string(start_line_byte..range.start).chars().count();
                            let end_col = doc.slice_to_string(end_line_byte..range.end).chars().count();
                            let line_strings = Self::table_lines_slice(doc, t1);
                            let lines_ref: Vec<&str> = line_strings.iter().map(|s| s.as_str()).collect();
                            let r1 = t1.row_of_line(start_line).unwrap_or(0);
                            let r2 = t1.row_of_line(end_line).unwrap_or(t1.row_count().saturating_sub(1));
                            let c1 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(t1, &lines_ref, start_line)), start_col);
                            let c2 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(t1, &lines_ref, end_line)), end_col);
                            if r1 != r2 || c1 != c2 {
                                let rect = crate::table::CellRect::new(r1, c1, r2, c2);
                                let model = crate::table::TableModel::parse(t1, &lines_ref);
                                Some(model.to_tsv(rect.row_start..rect.row_end, rect.col_start..rect.col_end))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let strip_tags = self.ctrl_preview_selection || !self.edit_mode;
            if let Some(tsv) = copied_tsv {
                let to_copy = if strip_tags {
                    crate::markdown::strip_markdown_tags(&tsv)
                } else {
                    tsv
                };
                cx.write_to_clipboard(ClipboardItem::new_string(to_copy));
                return;
            } else if let Some(text) = self.state.active_selected_text() {
                let to_copy = if strip_tags {
                    crate::markdown::strip_markdown_tags(&text)
                } else {
                    text
                };
                if !strip_tags {
                    if let Some(meta) = self.attachment_clipboard_metadata(&to_copy, false) {
                        cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(to_copy, meta));
                        return;
                    }
                }
                cx.write_to_clipboard(ClipboardItem::new_string(to_copy));
                return;
            }
        }

        // Ctrl + X (Cut)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("x") || keystroke.key == "ч" || keystroke.key == "Ч") {
            if let Some(text) = self.state.active_selected_text() {
                let meta = if self.edit_mode && !self.ctrl_preview_selection {
                    self.attachment_clipboard_metadata(&text, true)
                } else {
                    None
                };
                match meta {
                    Some(meta) => cx.write_to_clipboard(
                        ClipboardItem::new_string_with_json_metadata(text, meta),
                    ),
                    None => cx.write_to_clipboard(ClipboardItem::new_string(text)),
                }
                self.state.active_delete_selection();
                cx.notify();
            }
            self.schedule_auto_save(cx);
            return;
        }

        // Alt + Z (Toggle word wrap)
        if keystroke.modifiers.alt && (keystroke.key.eq_ignore_ascii_case("z") || keystroke.key == "я" || keystroke.key == "Я") {
            self.toggle_word_wrap(cx);
            return;
        }

        // Ctrl + / (Toggle raw source of the active table)
        if ctrl && (keystroke.key == "/" || keystroke.key == "?" || keystroke.key == "." || keystroke.key == "ю" || keystroke.key == "Ю") {
            if self.state.active_toggle_raw_table() {
                cx.notify();
                return;
            }
        }

        // Ctrl + V (Paste)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("v") || keystroke.key == "м" || keystroke.key == "М") {
            // A raster on the clipboard is an image, even when the same payload
            // also carries text (browsers, Paint). GPUI sees only its own
            // registered PNG/GIF/JFIF formats; CF_DIB (screenshots) is read by
            // the platform adapter instead.
            if self.paste_clipboard_image_aware(cx) {
                return;
            }
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    if let Some(paths) = AppView::clipboard_image_files(&text) {
                        if self.import_image_files(paths, cx) {
                            return;
                        }
                    }
                    if text.contains("![[") && self.copy_pasted_references(&text, None, cx) {
                        return;
                    }
                    let text = crate::document_buffer::normalize_newlines(&text).into_owned();
                    enum PasteAction {
                        ApplyEdit(crate::table::LineEdit, Option<(usize, usize)>),
                        InsertText(String),
                        Deny,
                    }

                    let paste_action = if let Some((view, doc)) = self.state.active_view_and_doc() {
                        let (cursor_line, cursor_col) = view.line_col(doc);
                        let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
                        if let Some(tbl) = view.grid_table_at(doc, &tables, cursor_line, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                            if cursor_line == tbl.sep_line() {
                                PasteAction::Deny
                            } else if text.contains('\t') {
                                let line_strings = Self::table_lines_slice(doc, tbl);
                                let lines_ref: Vec<&str> = line_strings.iter().map(|s| s.as_str()).collect();
                                let row = tbl.row_of_line(cursor_line).unwrap_or(0);
                                let cells = crate::table::split_row(crate::table::get_table_line(tbl, &lines_ref, cursor_line));
                                let col = crate::table::cell_index_at(&cells, cursor_col);
                                let mut model = crate::table::TableModel::parse(tbl, &lines_ref);
                                if let Some((max_r, max_c)) = model.paste_tsv(row, col, &text) {
                                    let edit = crate::table::rewrite(tbl, &model, false, &crate::table::char_width);
                                    let target_line = tbl.line_of_row(max_r);
                                    let new_row_str = &edit.new_lines[if max_r == 0 { 0 } else { max_r + 1 }];
                                    let cells_new = crate::table::split_row(new_row_str);
                                    let target_col = cells_new.get(max_c).map(|c| c.content.end).unwrap_or(2);
                                    PasteAction::ApplyEdit(edit, Some((target_line, target_col)))
                                } else {
                                    PasteAction::InsertText(crate::table::sanitize_cell_input(&text))
                                }
                            } else {
                                PasteAction::InsertText(crate::table::sanitize_cell_input(&text))
                            }
                        } else {
                            PasteAction::InsertText(text)
                        }
                    } else {
                        PasteAction::InsertText(text)
                    };

                    match paste_action {
                        PasteAction::ApplyEdit(edit, target_cursor) => {
                            self.state.active_apply_table_line_edit(&edit, target_cursor);
                            self.schedule_auto_save(cx);
                            cx.notify();
                            return;
                        }
                        PasteAction::InsertText(s) => {
                            self.state.active_insert_text(&s);
                            cx.notify();
                        }
                        PasteAction::Deny => {
                            return;
                        }
                    }
                }
            }
            self.schedule_auto_save(cx);
            return;
        }

        if self.state.active_doc_id().is_none() {
            return;
        }

        // Grid Table Navigation and Guarded Edits
        enum TableKeyAction {
            SetCursor(usize, usize),
            ApplyEdit(crate::table::LineEdit, Option<(usize, usize)>),
            InsertText(String),
            Deny,
            PassThrough,
        }

        let table_action = if let Some((view, doc)) = self.state.active_view_and_doc() {
            let (cursor_line, cursor_col) = view.line_col(doc);
            let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
            if let Some(tbl) = view.grid_table_at(doc, &tables, cursor_line, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                let line_str = doc.line_text(cursor_line);
                let cells = crate::table::split_row(&line_str);
                let current_row_cell = tbl
                    .row_of_line(cursor_line)
                    .map(|row| (row, crate::table::cell_index_at(&cells, cursor_col).min(tbl.col_count().saturating_sub(1))));
                // Only the target row's text is read: navigation must not clone the
                // whole table on every keypress.
                let cursor_at = |row: usize, cell: usize| -> TableKeyAction {
                    let target_l = tbl.line_of_row(row);
                    let target_text = doc.line_text(target_l);
                    let target_c = crate::table::split_row(&target_text)
                        .get(cell)
                        .map(|c| c.content.start)
                        .unwrap_or_else(|| target_text.chars().count());
                    TableKeyAction::SetCursor(target_l, target_c)
                };
                // The full table is only materialised when a row is appended.
                let append_row = |cell: usize| -> TableKeyAction {
                    let line_strings = Self::table_lines_slice(doc, tbl);
                    let lines_ref: Vec<&str> = line_strings.iter().map(|s| s.as_str()).collect();
                    let mut model = crate::table::TableModel::parse(tbl, &lines_ref);
                    model.insert_row(model.rows.len());
                    let edit = crate::table::rewrite(tbl, &model, false, &crate::table::char_width);
                    let target_l = edit.first_line + edit.new_lines.len() - 1;
                    let new_cells = crate::table::split_row(&edit.new_lines[edit.new_lines.len() - 1]);
                    let target_c = new_cells.get(cell).map(|c| c.content.start).unwrap_or(2);
                    TableKeyAction::ApplyEdit(edit, Some((target_l, target_c)))
                };

                match keystroke.key.as_str() {
                    "tab" => {
                        if let Some((row, cell)) = current_row_cell {
                            match crate::table::tab_from(tbl, row, cell, shift) {
                                crate::table::Nav::To { row, cell } => {
                                    cursor_at(row, cell)
                                }
                                crate::table::Nav::AppendRow { cell } => append_row(cell),
                                crate::table::Nav::Exit => {
                                    if tbl.first_line > 0 {
                                        TableKeyAction::SetCursor(tbl.first_line - 1, 0)
                                    } else {
                                        TableKeyAction::Deny
                                    }
                                }
                            }
                        } else {
                            TableKeyAction::Deny
                        }
                    }
                    "enter" => {
                        if let Some((row, cell)) = current_row_cell {
                            match crate::table::enter_from(tbl, row, cell, shift) {
                                crate::table::Nav::To { row, cell } => {
                                    cursor_at(row, cell)
                                }
                                crate::table::Nav::AppendRow { cell } => append_row(cell),
                                crate::table::Nav::Exit => {
                                    if tbl.first_line > 0 {
                                        TableKeyAction::SetCursor(tbl.first_line - 1, 0)
                                    } else {
                                        TableKeyAction::Deny
                                    }
                                }
                            }
                        } else {
                            TableKeyAction::Deny
                        }
                    }
                    "backspace" => {
                        if cursor_line == tbl.sep_line() {
                            TableKeyAction::Deny
                        } else if let Some(range) = view.selection_range(doc.len()) {
                            let start_line = doc.line_of_byte(range.start);
                            let end_line = doc.line_of_byte(range.end);
                            if start_line == end_line && start_line == cursor_line {
                                let line_start_byte = doc.line_start_byte(cursor_line);
                                let start_col = doc.slice_to_string(line_start_byte..range.start).chars().count();
                                let end_col = doc.slice_to_string(line_start_byte..range.end).chars().count();
                                if crate::table::guard_edit(tbl, cursor_line, &line_str, start_col..end_col) == crate::table::EditGuard::Deny {
                                    TableKeyAction::Deny
                                } else {
                                    TableKeyAction::PassThrough
                                }
                            } else if start_line != end_line && tbl.contains(start_line) && tbl.contains(end_line) {
                                let start_line_byte = doc.line_start_byte(start_line);
                                let end_line_byte = doc.line_start_byte(end_line);
                                let start_col = doc.slice_to_string(start_line_byte..range.start).chars().count();
                                let end_col = doc.slice_to_string(end_line_byte..range.end).chars().count();
                                let line_strings = Self::table_lines_slice(doc, tbl);
                                let lines_ref: Vec<&str> = line_strings.iter().map(|s| s.as_str()).collect();
                                let r1 = tbl.row_of_line(start_line).unwrap_or(0);
                                let r2 = tbl.row_of_line(end_line).unwrap_or(tbl.row_count().saturating_sub(1));
                                let c1 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(tbl, &lines_ref, start_line)), start_col);
                                let c2 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(tbl, &lines_ref, end_line)), end_col);
                                let rect = crate::table::CellRect::new(r1, r2, c1, c2);
                                let mut model = crate::table::TableModel::parse(tbl, &lines_ref);
                                model.clear_rect(&rect);
                                let edit = crate::table::rewrite(tbl, &model, false, &crate::table::char_width);
                                TableKeyAction::ApplyEdit(edit, Some((cursor_line, cursor_col)))
                            } else {
                                TableKeyAction::PassThrough
                            }
                        } else {
                            if cursor_col == 0 || crate::table::guard_edit(tbl, cursor_line, &line_str, cursor_col - 1..cursor_col) == crate::table::EditGuard::Deny {
                                TableKeyAction::Deny
                            } else {
                                TableKeyAction::PassThrough
                            }
                        }
                    }
                    "delete" => {
                        if cursor_line == tbl.sep_line() {
                            TableKeyAction::Deny
                        } else if let Some(range) = view.selection_range(doc.len()) {
                            let start_line = doc.line_of_byte(range.start);
                            let end_line = doc.line_of_byte(range.end);
                            if start_line == end_line && start_line == cursor_line {
                                let line_start_byte = doc.line_start_byte(cursor_line);
                                let start_col = doc.slice_to_string(line_start_byte..range.start).chars().count();
                                let end_col = doc.slice_to_string(line_start_byte..range.end).chars().count();
                                if crate::table::guard_edit(tbl, cursor_line, &line_str, start_col..end_col) == crate::table::EditGuard::Deny {
                                    TableKeyAction::Deny
                                } else {
                                    TableKeyAction::PassThrough
                                }
                            } else if start_line != end_line && tbl.contains(start_line) && tbl.contains(end_line) {
                                let start_line_byte = doc.line_start_byte(start_line);
                                let end_line_byte = doc.line_start_byte(end_line);
                                let start_col = doc.slice_to_string(start_line_byte..range.start).chars().count();
                                let end_col = doc.slice_to_string(end_line_byte..range.end).chars().count();
                                let line_strings = Self::table_lines_slice(doc, tbl);
                                let lines_ref: Vec<&str> = line_strings.iter().map(|s| s.as_str()).collect();
                                let r1 = tbl.row_of_line(start_line).unwrap_or(0);
                                let r2 = tbl.row_of_line(end_line).unwrap_or(tbl.row_count().saturating_sub(1));
                                let c1 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(tbl, &lines_ref, start_line)), start_col);
                                let c2 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(tbl, &lines_ref, end_line)), end_col);
                                let rect = crate::table::CellRect::new(r1, r2, c1, c2);
                                let mut model = crate::table::TableModel::parse(tbl, &lines_ref);
                                model.clear_rect(&rect);
                                let edit = crate::table::rewrite(tbl, &model, false, &crate::table::char_width);
                                TableKeyAction::ApplyEdit(edit, Some((cursor_line, cursor_col)))
                            } else {
                                TableKeyAction::PassThrough
                            }
                        } else {
                            if crate::table::guard_edit(tbl, cursor_line, &line_str, cursor_col..cursor_col + 1) == crate::table::EditGuard::Deny {
                                TableKeyAction::Deny
                            } else {
                                TableKeyAction::PassThrough
                            }
                        }
                    }
                    "left" => {
                        if !shift {
                            let cell = crate::table::cell_index_at(&cells, cursor_col);
                            if let Some(c) = cells.get(cell) {
                                if cursor_col <= c.content.start && cell > 0 {
                                    let prev_c = &cells[cell - 1];
                                    TableKeyAction::SetCursor(cursor_line, prev_c.content.end)
                                } else {
                                    TableKeyAction::PassThrough
                                }
                            } else {
                                TableKeyAction::PassThrough
                            }
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                    "right" => {
                        if !shift {
                            let cell = crate::table::cell_index_at(&cells, cursor_col);
                            if let Some(c) = cells.get(cell) {
                                if cursor_col >= c.content.end && cell + 1 < cells.len() {
                                    let next_c = &cells[cell + 1];
                                    TableKeyAction::SetCursor(cursor_line, next_c.content.start)
                                } else {
                                    TableKeyAction::PassThrough
                                }
                            } else {
                                TableKeyAction::PassThrough
                            }
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                    "home" => {
                        let cell = crate::table::cell_index_at(&cells, cursor_col);
                        if let Some(c) = cells.get(cell) {
                            TableKeyAction::SetCursor(cursor_line, c.content.start)
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                    "end" => {
                        let cell = crate::table::cell_index_at(&cells, cursor_col);
                        if let Some(c) = cells.get(cell) {
                            TableKeyAction::SetCursor(cursor_line, c.content.end)
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                    "up" => {
                        if !shift {
                            let cell = crate::table::cell_index_at(&cells, cursor_col);
                            if let Some(row) = tbl.row_of_line(cursor_line) {
                                if row > 0 {
                                    cursor_at(row - 1, cell)
                                } else if tbl.first_line > 0 {
                                    TableKeyAction::SetCursor(tbl.first_line - 1, 0)
                                } else {
                                    TableKeyAction::PassThrough
                                }
                            } else {
                                TableKeyAction::PassThrough
                            }
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                    "down" => {
                        if !shift {
                            let cell = crate::table::cell_index_at(&cells, cursor_col);
                            if let Some(row) = tbl.row_of_line(cursor_line) {
                                if row + 1 < tbl.row_count() {
                                    cursor_at(row + 1, cell)
                                } else {
                                    let next_line = tbl.last_line + 1;
                                    if next_line < doc.line_index.lines() {
                                        TableKeyAction::SetCursor(next_line, 0)
                                    } else {
                                        TableKeyAction::PassThrough
                                    }
                                }
                            } else {
                                TableKeyAction::PassThrough
                            }
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                    _ => {
                        if !ctrl && !keystroke.modifiers.alt && !keystroke.modifiers.platform {
                            if cursor_line == tbl.sep_line() {
                                TableKeyAction::Deny
                            } else {
                                let typed_str = keystroke.key_char.as_deref().unwrap_or(keystroke.key.as_str());
                                if typed_str == "|" {
                                    TableKeyAction::InsertText("\\|".into())
                                } else {
                                    TableKeyAction::PassThrough
                                }
                            }
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                }
            } else {
                TableKeyAction::PassThrough
            }
        } else {
            TableKeyAction::PassThrough
        };

        match table_action {
            TableKeyAction::SetCursor(line, col) => {
                self.state.active_set_cursor_line_col(line, col);
                self.refresh_view_after_cursor_move();
                cx.notify();
                return;
            }
            TableKeyAction::ApplyEdit(edit, target_cursor) => {
                self.state.active_apply_table_line_edit(&edit, target_cursor);
                self.schedule_auto_save(cx);
                cx.notify();
                return;
            }
            TableKeyAction::InsertText(s) => {
                self.state.active_insert_text(&s);
                self.schedule_auto_save(cx);
                cx.notify();
                return;
            }
            TableKeyAction::Deny => {
                return;
            }
            TableKeyAction::PassThrough => {}
        }

        if keystroke.key == "backspace" || keystroke.key == "delete" {
            let edit_opt = if let Some((view, doc)) = self.state.active_view_and_doc() {
                if let Some(range) = view.selection_range(doc.len()) {
                    let start_line = doc.line_of_byte(range.start);
                    let end_line = doc.line_of_byte(range.end);
                    // Windowed scan only: a full-document scan here froze the
                    // first Delete on large notes opened without a full index.
                    let tables = doc.ensure_table_index_for_window(start_line, end_line);
                    if let (Some(t1), Some(t2)) = (tables.at(start_line), tables.at(end_line)) {
                        if t1.first_line == t2.first_line && start_line != end_line && view.grid_table_at(doc, &tables, start_line, start_line, self.ctrl_preview_selection || !self.edit_mode).is_some() {
                            let start_line_byte = doc.line_start_byte(start_line);
                            let end_line_byte = doc.line_start_byte(end_line);
                            let start_col = doc.slice_to_string(start_line_byte..range.start).chars().count();
                            let end_col = doc.slice_to_string(end_line_byte..range.end).chars().count();
                            let line_strings = Self::table_lines_slice(doc, t1);
                            let lines_ref: Vec<&str> = line_strings.iter().map(|s| s.as_str()).collect();
                            let r1 = t1.row_of_line(start_line).unwrap_or(0);
                            let r2 = t1.row_of_line(end_line).unwrap_or(t1.row_count().saturating_sub(1));
                            let c1 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(t1, &lines_ref, start_line)), start_col);
                            let c2 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(t1, &lines_ref, end_line)), end_col);
                            let rect = crate::table::CellRect::new(r1, c1, r2, c2);
                            let mut model = crate::table::TableModel::parse(t1, &lines_ref);
                            for r in rect.row_start..rect.row_end {
                                for c in rect.col_start..rect.col_end {
                                    if let Some(row) = model.rows.get_mut(r) {
                                        if let Some(cell) = row.get_mut(c) {
                                            cell.clear();
                                        }
                                    }
                                }
                            }
                            let edit = crate::table::rewrite(t1, &model, false, &crate::table::char_width);
                            Some((t1.first_line, edit))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((first_line, edit)) = edit_opt {
                self.table_layouts.borrow_mut().remove(&first_line);
                self.state.active_apply_table_line_edit(&edit, None);
                if let Some(v) = self.state.active_view_mut() {
                    v.selection_anchor = None;
                }
                self.schedule_auto_save(cx);
                cx.notify();
                return;
            }
        }

        // A text edit or a cursor/selection move must not just repaint once:
        // one notify per keystroke renders the editor at key-repeat rate
        // (~30 fps), and the caret/selection visibly *step* between frames —
        // plus the scroll reveal and the render-side clamp then fight across
        // frames and the selection looks one event behind the caret.
        //
        // The settle frame, however, is only worth arming when the keystroke
        // actually moved the scroll: the reveal below already wrote the final
        // scroll position into the very frame that paints the edit, so a plain
        // in-line insert has nothing new for a second frame to paint — and at
        // key-repeat the doubled cadence (~60 fps vs ~30) is most of the GPU
        // burn (measured 2026-10-21: ~8% GPU at one frame per char, ~18–20% at
        // two). Scrolling keystrokes — typing at the viewport edge, cursor
        // moves that leave the viewport — still get their settle frame.
        let version_before = self.state.active_document().map(|d| d.version);
        let anchor_before = self.state.active_view().and_then(|v| v.selection_anchor);
        let cursor_before = self.state.active_view().map(|v| v.cursor_offset);
        let scroll_before = self.current_scroll_top_f64();
        match keystroke.key.as_str() {
            "backspace" => self.state.active_delete_backwards(),
            "delete" => self.state.active_delete_forwards(),
            "enter" => self.state.active_insert_newline_auto_indent(),
            "tab" => self.state.active_insert_text("    "),
            "space" => self.state.active_insert_text(" "),
            "left" => self.state.active_move_cursor_left(shift),
            "right" => self.state.active_move_cursor_right(shift),
            "up" => self.move_cursor_up_visual(shift),
            "down" => self.move_cursor_down_visual(shift),
            "home" => self.move_cursor_home_visual(shift),
            "end" => self.move_cursor_end_visual(shift),
            _ => {
                if !ctrl && !keystroke.modifiers.alt && !keystroke.modifiers.platform {
                    if let Some(ch) = &keystroke.key_char {
                        self.state.active_insert_text(ch);
                    } else if keystroke.key.chars().count() == 1 {
                        self.state.active_insert_text(&keystroke.key);
                    }
                }
            }
        }
        let version_after = self.state.active_document().map(|d| d.version);
        let state_changed = version_before != version_after
            || anchor_before != self.state.active_view().and_then(|v| v.selection_anchor)
            || cursor_before != self.state.active_view().map(|v| v.cursor_offset);
        if self.state.active_document().map(|d| d.is_dirty).unwrap_or(false) {
            self.schedule_auto_save(cx);
        }
        // Plugins read the buffer synchronously from the snapshot: refresh it
        // on the single exit point whenever text, cursor or selection moved.
        if state_changed {
            self.state.publish_snapshot();
        }
        // Refresh the rows before revealing: every edit above marked the
        // cache dirty, and the lookup below must see the new rows already —
        // otherwise a row created at the very bottom (Enter) stays hidden
        // under the status bar until the next frame's rebuild.
        self.refresh_view_after_cursor_move();
        // Decided after the reveal ran: it is the only thing here that moves
        // the scroll, and it writes the final position directly, so the settle
        // frame is needed exactly when that write happened.
        if state_changed && (self.current_scroll_top_f64() - scroll_before).abs() > 0.5 {
            self.layout_settle_frames.set(1);
        }
        cx.notify();
    }

    pub(super) fn render_navigation_bar(
        &self,
        doc_override: Option<&crate::document_buffer::DocumentBuffer>,
        leaf_id: Option<crate::workspace::LeafId>,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let is_empty_tab = leaf_id
            .and_then(|lid| self.state.leaves.get(&lid))
            .and_then(|l| l.active_tab())
            .map(|t| matches!(t.content, crate::workspace::TabContent::Empty))
            .unwrap_or(doc_override.is_none() && self.state.active_doc_id().is_none() && self.state.active_empty_tab.is_some());

        let can_go_back = self.state.can_navigate_back();
        let can_go_forward = self.state.can_navigate_forward();
        let full_path_opt = if let Some(doc) = doc_override {
            Some(doc.path.clone())
        } else if let Some(lid) = leaf_id {
            self.state
                .leaves
                .get(&lid)
                .and_then(|l| l.active_tab())
                .and_then(|t| match &t.content {
                    crate::workspace::TabContent::Document { id } => {
                        self.state.document_path(*id).map(|p| p.to_path_buf())
                    }
                    _ => None,
                })
        } else {
            self.state.active_document().map(|d| d.path.clone())
                .or_else(|| self.state.active_doc_id().and_then(|id| self.state.document_path(id).map(|p| p.to_path_buf())))
        };
        struct NavBreadcrumb {
            name: String,
            path: Option<PathBuf>,
            is_folder: bool,
        }

        let breadcrumbs = if is_empty_tab {
            vec![NavBreadcrumb {
                name: self.t("tabs.newTab"),
                path: None,
                is_folder: false,
            }]
        } else if let Some(ref doc_path) = full_path_opt {
            if let Some(ref ws) = self.state.workspace_path {
                if let Ok(rel) = doc_path.strip_prefix(ws) {
                    let mut list = Vec::new();
                    list.push(NavBreadcrumb {
                        name: self.state.workspace_name.clone(),
                        path: Some(ws.clone()),
                        is_folder: true,
                    });
                    let mut curr = ws.clone();
                    let comps: Vec<_> = rel.components().collect();
                    let total = comps.len();
                    for (i, comp) in comps.into_iter().enumerate() {
                        curr.push(comp);
                        let comp_str = comp.as_os_str().to_string_lossy().to_string();
                        if comp_str.is_empty() {
                            continue;
                        }
                        let is_last = i == total.saturating_sub(1);
                        if is_last {
                            let mut name = comp_str;
                            if let Some(stripped) = name.strip_suffix(".md") {
                                name = stripped.to_string();
                            } else if let Some(stripped) = name.strip_suffix(".markdown") {
                                name = stripped.to_string();
                            }
                            list.push(NavBreadcrumb {
                                name,
                                path: Some(curr.clone()),
                                is_folder: false,
                            });
                        } else {
                            list.push(NavBreadcrumb {
                                name: comp_str,
                                path: Some(curr.clone()),
                                is_folder: true,
                            });
                        }
                    }
                    list
                } else {
                    let title = doc_override
                        .map(|d| d.title.clone())
                        .or_else(|| {
                            leaf_id
                                .and_then(|lid| self.state.leaves.get(&lid))
                                .and_then(|l| l.active_tab())
                                .and_then(|t| match &t.content {
                                    crate::workspace::TabContent::Document { id } => {
                                        self.state.document_title(*id).map(|s| s.to_string())
                                    }
                                    _ => None,
                                })
                        })
                        .or_else(|| self.state.active_document().map(|d| d.title.clone()))
                        .unwrap_or_else(|| self.t("tabs.newTab"));
                    vec![NavBreadcrumb {
                        name: title,
                        path: Some(doc_path.clone()),
                        is_folder: false,
                    }]
                }
            } else {
                let title = doc_override
                    .map(|d| d.title.clone())
                    .or_else(|| {
                        leaf_id
                            .and_then(|lid| self.state.leaves.get(&lid))
                            .and_then(|l| l.active_tab())
                            .and_then(|t| match &t.content {
                                crate::workspace::TabContent::Document { id } => {
                                    self.state.document_title(*id).map(|s| s.to_string())
                                }
                                _ => None,
                            })
                    })
                    .or_else(|| self.state.active_document().map(|d| d.title.clone()))
                    .unwrap_or_else(|| self.t("tabs.newTab"));
                vec![NavBreadcrumb {
                    name: title,
                    path: Some(doc_path.clone()),
                    is_folder: false,
                }]
            }
        } else {
            vec![NavBreadcrumb {
                name: self.t("tabs.newTab"),
                path: None,
                is_folder: false,
            }]
        };
        let is_dirty = if let Some(doc) = doc_override {
            doc.is_dirty
        } else if let Some(lid) = leaf_id {
            self.state
                .leaves
                .get(&lid)
                .and_then(|l| l.active_tab())
                .and_then(|t| match &t.content {
                    crate::workspace::TabContent::Document { id } => Some(self.state.is_document_dirty(*id)),
                    _ => None,
                })
                .unwrap_or(false)
        } else {
            self.state.active_document().map(|d| d.is_dirty).unwrap_or(false)
        };
        let is_editing = self.edit_mode;
        let disabled_nav_color = if theme.is_dark {
            gpui::rgba(0x7f849c44)
        } else {
            gpui::rgba(0x5c5f7744)
        };

        div()
            .id("editor-navigation-bar")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(40.0))
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .px_3()
            .bg(theme.bg_editor)
            // Left: Back/Forward buttons + Breadcrumbs
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .min_w_0()
                    .overflow_hidden()
                    // Back button
                    .child(
                        div()
                            .id("btn-nav-back")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .when(can_go_back, |d| {
                                d.cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_ARROW_LEFT, 16.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.navigate_back(window, cx);
                                    }))
                            })
                            .when(!can_go_back, |d| {
                                d.cursor_default()
                                    .child(icons::icon(icons::ICON_ARROW_LEFT, 16.0, disabled_nav_color))
                            }),
                    )
                    // Forward button
                    .child(
                        div()
                            .id("btn-nav-forward")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .when(can_go_forward, |d| {
                                d.cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_ARROW_RIGHT, 16.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.navigate_forward(window, cx);
                                    }))
                            })
                            .when(!can_go_forward, |d| {
                                d.cursor_default()
                                    .child(icons::icon(icons::ICON_ARROW_RIGHT, 16.0, disabled_nav_color))
                            }),
                    )
                    // Spacer between arrows and breadcrumbs
                    .child(div().w(px(10.0)))
                    // Breadcrumbs trail
                    .child({
                        let mut crumbs_bar = div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .min_w_0()
                            .overflow_hidden();

                        let count = breadcrumbs.len();
                        for (idx, crumb) in breadcrumbs.iter().enumerate() {
                            if idx > 0 {
                                crumbs_bar = crumbs_bar.child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.text_muted)
                                        .px_1p5()
                                        .flex_shrink_0()
                                        .child("/"),
                                );
                            }
                            let is_last = idx == count.saturating_sub(1);
                            let is_root_folder = idx == 0 && crumb.is_folder;
                            let is_folder = crumb.is_folder;
                            let item_path = crumb.path.clone();

                            let crumb_color = if is_last && is_dirty {
                                theme.text_primary
                            } else {
                                theme.text_muted
                            };

                            let mut item_elem = div()
                                .id(SharedString::from(format!("nav-crumb-{}", idx)))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1p5()
                                .min_w_0()
                                .px_1p5()
                                .py_0p5()
                                .rounded_md()
                                .text_color(crumb_color);

                            if is_root_folder {
                                item_elem = item_elem.child(
                                    div()
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .child(icons::icon(icons::ICON_FOLDER, 15.0, theme.accent)),
                                );
                            }

                            item_elem = item_elem.child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .truncate()
                                    .child(crumb.name.clone()),
                            );

                            if is_folder {
                                item_elem = item_elem
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary));
                                if let Some(path) = item_path {
                                    item_elem = item_elem.on_click(cx.listener(move |this, _, _, cx| {
                                        this.select_sidebar_folder(path.clone(), true, cx);
                                    }));
                                }
                            } else if let Some(path) = item_path {
                                item_elem = item_elem
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.sidebar_mode = crate::views::sidebar::SidebarMode::Files;
                                        this.sidebar_collapsed = false;
                                        this.selected_sidebar_folder = None;
                                        this.state.reveal_path_in_tree(&path);
                                        this.need_scroll_sidebar_to_active.set(true);
                                        cx.notify();
                                    }));
                            }

                            crumbs_bar = crumbs_bar.child(item_elem);
                        }

                        if let Some(ref full_path) = full_path_opt {
                            if !crate::state::is_empty_tab_path(full_path) {
                                let path_to_copy = full_path.to_string_lossy().to_string();
                                crumbs_bar = crumbs_bar.child(
                                    div()
                                        .id("btn-copy-full-path")
                                        .flex()
                                        .flex_shrink_0()
                                        .items_center()
                                        .justify_center()
                                        .w(px(22.0))
                                        .h(px(22.0))
                                        .ml_1p5()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .hover(|h| h.bg(theme.bg_hover))
                                        .child(icons::icon(
                                            icons::ICON_COPY,
                                            13.0,
                                            theme.text_muted,
                                        ))
                                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(path_to_copy.clone()));
                                            this.state.status_text = this.t_fmt("sidebar.copy_absolute_path_msg", &[&path_to_copy]);
                                            cx.notify();
                                        })),
                                );
                            }
                        }

                        crumbs_bar
                    }),
            )
            // Right: Obsidian-style action buttons
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .flex_shrink_0()
                    // Bookmark indicator / toggle
                    .when(!is_empty_tab && full_path_opt.is_some(), |row| {
                        let is_bookmarked = full_path_opt
                            .as_ref()
                            .map(|p| self.is_file_bookmarked(p))
                            .unwrap_or(false);
                        let p_click = full_path_opt.clone();

                        row.child(
                            div()
                                .id("btn-note-bookmark")
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(28.0))
                                .h(px(28.0))
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(if is_bookmarked {
                                    // Purple ribbon icon matching Image 2
                                    icons::icon(icons::ICON_BOOKMARK_FILLED, 16.0, rgb(0x8b5cf6))
                                } else {
                                    icons::icon(icons::ICON_BOOKMARK, 16.0, theme.text_muted)
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(ref p) = p_click {
                                        this.toggle_file_bookmark(p, cx);
                                    }
                                })),
                        )
                    })
                    // Toggle wrap
                    .child(
                        div()
                            .id("btn-toggle-wrap")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                icons::ICON_CODE,
                                15.0,
                                if self.word_wrap { theme.accent } else { theme.text_muted },
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_word_wrap(cx);
                            })),
                    )
                    // Toggle reader / edit view
                    .child(
                        div()
                            .id("btn-toggle-reading-mode")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                icons::ICON_BOOK_OPEN,
                                16.0,
                                if !is_editing { theme.accent } else { theme.text_muted },
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit_mode = !this.edit_mode;
                                let current_scroll = this.current_scroll_top();
                                this.state.snapshot_active_note_settings(current_scroll, this.edit_mode, this.word_wrap, this.source_mode);
                                if this.edit_mode {
                                    this.reset_editor_cursor_blink(cx);
                                }
                                cx.notify();
                            })),
                    )
                    // More options (3 vertical dots - opens Obsidian-style context menu)
                    .child({
                        let is_menu_open = self.more_options_menu.is_open;
                        let target_path_left = full_path_opt.clone();
                        let target_path_right = full_path_opt.clone();
                        let target_leaf_for_click = leaf_id;
                        div()
                            .id("btn-more-options")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .cursor_pointer()
                            .when(is_menu_open, |d| d.bg(theme.bg_hover))
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                icons::ICON_MORE_VERTICAL,
                                16.0,
                                if is_menu_open { theme.text_primary } else { theme.text_muted },
                            ))
                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                let x: f32 = event.position.x.into();
                                let y: f32 = event.position.y.into();
                                if this.more_options_menu.is_open {
                                    this.close_more_options_menu(cx);
                                } else {
                                    this.open_more_options_menu(target_path_left.clone(), target_leaf_for_click, x, y, cx);
                                }
                            }))
                            .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                let x: f32 = event.position.x.into();
                                let y: f32 = event.position.y.into();
                                this.open_more_options_menu(target_path_right.clone(), target_leaf_for_click, x, y, cx);
                            }))
                    }),
            )
    }
}
