use super::*;

impl AppView {
    pub(crate) fn commit_title_rename_internal(&mut self) -> bool {
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return false;
        };
        let Some(input) = view.title_input.take() else {
            return false;
        };
        let new_title = input.text.trim();
        let current_path = doc.path.clone();
        let current_title = doc.title.clone();

        if new_title.is_empty() || new_title == current_title {
            return false;
        }

        match self.state.rename_note(&current_path, new_title) {
            Ok(_) => true,
            Err(err) => {
                if let Some(view) = self.state.active_view_mut() {
                    view.title_input = Some(input);
                }
                self.state.status_text = format!("Rename failed: {}", err);
                false
            }
        }
    }

    pub fn commit_title_rename(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.commit_title_rename_internal() {
            cx.notify();
        }
    }

    pub(super) fn handle_title_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        let ctrl = event.keystroke.modifiers.control;
        let shift = event.keystroke.modifiers.shift;
        let alt = event.keystroke.modifiers.alt;

        if key == "enter" || key == "tab" {
            self.commit_title_rename(window, cx);
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        if key == "escape" {
            if let Some(view) = self.state.active_view_mut() {
                view.cancel_title_edit();
            }
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        if key == "down" {
            self.commit_title_rename(window, cx);
            if let Some(view) = self.state.active_view_mut() {
                view.cursor_offset = 0;
                view.selection_anchor = None;
            }
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        let Some(view) = self.state.active_view_mut() else {
            return;
        };
        let Some(ref mut input) = view.title_input else {
            return;
        };

        // Ctrl + A (Select All)
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            input.select_all();
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Ctrl + C (Copy selected text)
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some(selected) = input.selected_text() {
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
            }
            return;
        }

        // Ctrl + X (Cut selected text)
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some(selected) = input.selected_text() {
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
                input.delete_selection();
                self.reset_title_cursor_blink(cx);
                cx.notify();
            }
            return;
        }

        // Ctrl + S (Save note)
        if ctrl && (key.eq_ignore_ascii_case("s") || key == "ы" || key == "Ы") {
            self.commit_title_rename(window, cx);
            self.save_note(window, cx);
            return;
        }

        // Ctrl + V (Paste over selection or at cursor)
        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\n', '\r', '\t'];
                    let filtered: String = text
                        .chars()
                        .filter(|c| !invalid_chars.contains(c) && !c.is_control())
                        .collect();
                    if !filtered.is_empty() {
                        input.insert_text(&filtered);
                        self.reset_title_cursor_blink(cx);
                        cx.notify();
                    }
                }
            }
            return;
        }

        // Left Arrow
        if key == "left" {
            input.move_left(shift);
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Right Arrow
        if key == "right" {
            input.move_right(shift);
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Home
        if key == "home" {
            input.move_home(shift);
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // End
        if key == "end" {
            input.move_end(shift);
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Backspace
        if key == "backspace" {
            input.backspace();
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Delete
        if key == "delete" {
            input.delete();
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Normal character input
        if !ctrl && !alt && !event.keystroke.modifiers.platform {
            let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\n', '\r', '\t'];
            let text_to_insert = if key == "space" {
                Some(" ")
            } else if let Some(ch) = &event.keystroke.key_char {
                Some(ch.as_str())
            } else if key.chars().count() == 1 {
                Some(key.as_str())
            } else {
                None
            };

            if let Some(text) = text_to_insert {
                let filtered: String = text
                    .chars()
                    .filter(|c| !invalid_chars.contains(c) && !c.is_control())
                    .collect();
                if !filtered.is_empty() {
                    input.insert_text(&filtered);
                    self.reset_title_cursor_blink(cx);
                    cx.notify();
                }
            }
        }
    }

    pub(crate) fn first_matching_file(&self, nodes: &[FileNode]) -> Option<PathBuf> {
        let query = self.state.search_query.trim().to_lowercase();
        self.first_matching_file_internal(nodes, &query, false)
    }

    pub(super) fn first_matching_file_internal(&self, nodes: &[FileNode], query: &str, parent_matched: bool) -> Option<PathBuf> {
        for node in nodes {
            match node {
                FileNode::File { name, path } => {
                    if query.is_empty() || parent_matched || name.to_lowercase().contains(query) {
                        return Some(path.clone());
                    }
                }
                FileNode::Folder { name, children, .. } => {
                    let folder_matched = parent_matched || (!query.is_empty() && name.to_lowercase().contains(query));
                    if let Some(p) = self.first_matching_file_internal(children, query, folder_matched) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }
}
