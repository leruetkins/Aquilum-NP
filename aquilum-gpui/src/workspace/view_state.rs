use std::time::Instant;

use crate::document_buffer::{
    transform_offset, transform_selection_range, Bias, DocumentId, Edit, ViewportAnchor,
};
use crate::editor_window::EditorWindow;
use crate::workspace::TabId;

#[derive(Clone, Copy, Debug)]
pub struct WheelEaseState {
    pub start: f64,
    pub target: f64,
    pub t0: Instant,
}

#[derive(Clone, Debug)]
pub struct EditorViewState {
    pub tab_id: TabId,
    pub doc_id: DocumentId,
    pub cursor_offset: usize,
    pub selection_anchor: Option<usize>,
    pub viewport: ViewportAnchor,
    pub scroll_top: f64,
    pub wheel_ease: Option<WheelEaseState>,
    pub window: EditorWindow,
    pub above_lines: usize,
    pub below_lines: usize,
    pub above_h: f64,
    pub below_h: f64,
    pub est_rows_per_line: f64,
    pub est_row_h: f64,
    pub visual_cache_dirty: bool,
    pub is_mouse_selecting: bool,
    pub word_wrap: bool,
    pub edit_mode: bool,
    // File name input above the editor (R16): per tab, not in AppView
    pub title_edit_text: String,
    pub title_cursor_pos: usize,
    pub title_selection_anchor: Option<usize>,
    pub title_is_focused: bool,
}

impl EditorViewState {
    pub fn new(tab_id: TabId, doc_id: DocumentId) -> Self {
        Self {
            tab_id,
            doc_id,
            cursor_offset: 0,
            selection_anchor: None,
            viewport: ViewportAnchor::default(),
            scroll_top: 0.0,
            wheel_ease: None,
            window: EditorWindow::default(),
            above_lines: 0,
            below_lines: 0,
            above_h: 0.0,
            below_h: 0.0,
            est_rows_per_line: 1.0,
            est_row_h: 24.0,
            visual_cache_dirty: true,
            is_mouse_selecting: false,
            word_wrap: true,
            edit_mode: false,
            title_edit_text: String::new(),
            title_cursor_pos: 0,
            title_selection_anchor: None,
            title_is_focused: false,
        }
    }

    /// Handles `BufferEvent::Edited` from another view/panel according to Section 5.
    /// Invariant R5: Visible text does not move when edits occur outside or above the viewport.
    pub fn apply_external_edit(
        &mut self,
        edit: Edit,
        inserted_newlines: usize,
        removed_newlines: usize,
    ) {
        // 1. Transform viewport anchor
        let old_anchor_offset = self.viewport.offset;
        self.viewport.offset = transform_offset(self.viewport.offset, edit, Bias::Right);

        // 2. Transform cursor and selection
        if let Some(anchor) = self.selection_anchor {
            let (new_c, new_a) = transform_selection_range((self.cursor_offset, anchor), edit);
            self.cursor_offset = new_c;
            self.selection_anchor = Some(new_a);
        } else {
            self.cursor_offset = transform_offset(self.cursor_offset, edit, Bias::Left);
        }

        // 3. Invalidate visual cache for re-layout
        self.visual_cache_dirty = true;

        // 4. Update scroll_top and above/below estimates based on whether the edit was above the viewport
        if edit.start < old_anchor_offset {
            // Edit occurred above current viewport anchor.
            let line_diff = (inserted_newlines as isize) - (removed_newlines as isize);
            let h_diff = (line_diff as f64) * self.est_rows_per_line * self.est_row_h;

            self.scroll_top = (self.scroll_top + h_diff).max(0.0);
            self.above_h = (self.above_h + h_diff).max(0.0);
            if line_diff >= 0 {
                self.above_lines += line_diff as usize;
            } else {
                self.above_lines = self.above_lines.saturating_sub((-line_diff) as usize);
            }
        } else {
            // Edit occurred inside or below visible viewport: scroll_top stays EXACTLY the same!
            // Visible text does not shift by even 1px (R5).
        }
    }

    /// Handles a concrete `NoteEditEvent` from another active view/panel.
    /// Invariant R5: Visible text does not move when edits occur outside or above the viewport.
    /// Recalculates gutter line numbers, shifts viewport anchor and scroll_top without visible drift.
    pub fn apply_external_edit_detailed(&mut self, edit: &crate::state::NoteEditEvent) {
        let edit_obj = Edit {
            start: edit.at,
            old_len: edit.old_len,
            new_len: edit.new_len,
        };
        let line_diff = edit.line_diff;
        let byte_diff = (edit.new_len as isize) - (edit.old_len as isize);
        let line_h = self.est_rows_per_line * self.est_row_h;
        let h_diff = (line_diff as f64) * line_h;

        // 1. Transform cursor and selection
        if let Some(anchor) = self.selection_anchor {
            let (new_c, new_a) = transform_selection_range((self.cursor_offset, anchor), edit_obj);
            self.cursor_offset = new_c;
            self.selection_anchor = Some(new_a);
        } else {
            self.cursor_offset = transform_offset(self.cursor_offset, edit_obj, Bias::Left);
        }

        // 2. Viewport and window adjustments based on edit position
        if self.window.is_empty() {
            if edit.edit_line < self.viewport.offset {
                self.viewport.offset = (self.viewport.offset as isize + line_diff).max(0) as usize;
                self.scroll_top = (self.scroll_top + h_diff).max(0.0);
                self.above_h = (self.above_h + h_diff).max(0.0);
                if line_diff >= 0 {
                    self.above_lines += line_diff as usize;
                } else {
                    self.above_lines = self.above_lines.saturating_sub((-line_diff) as usize);
                }
            }
            self.visual_cache_dirty = true;
            return;
        }

        if edit.edit_line < self.window.start_line {
            // Edit occurred strictly above the window:
            // Text inside window does not change; only line numbers, byte bounds, anchor line,
            // above_lines, above_h and scroll_top shift by exactly line_diff / h_diff.
            self.viewport.offset = (self.viewport.offset as isize + line_diff).max(0) as usize;
            self.above_lines = (self.above_lines as isize + line_diff).max(0) as usize;
            self.above_h = (self.above_h + h_diff).max(0.0);
            self.scroll_top = (self.scroll_top + h_diff).max(0.0);
            self.window.shift_lines_and_bytes(line_diff, byte_diff);
            self.visual_cache_dirty = false;
        } else if edit.edit_line < self.window.end_line {
            // Edit occurred inside the window:
            if edit.edit_line < self.viewport.offset {
                self.viewport.offset = (self.viewport.offset as isize + line_diff).max(0) as usize;
                self.scroll_top = (self.scroll_top + h_diff).max(0.0);
                self.above_h = (self.above_h + h_diff).max(0.0);
                if line_diff >= 0 {
                    self.above_lines += line_diff as usize;
                } else {
                    self.above_lines = self.above_lines.saturating_sub((-line_diff) as usize);
                }
            }
            self.window = EditorWindow::default();
            self.visual_cache_dirty = true;
        } else {
            // Edit occurred strictly below the window:
            if line_diff >= 0 {
                self.below_lines += line_diff as usize;
            } else {
                self.below_lines = self.below_lines.saturating_sub((-line_diff) as usize);
            }
            self.below_h = (self.below_lines as f64) * line_h;
            self.visual_cache_dirty = false;
        }
    }

    #[inline]
    pub fn total_height(&self) -> f64 {
        self.above_h + self.window.total_height() + self.below_h
    }

    pub fn line_at_doc_y(&self, lines_count: usize, doc_y: f64) -> usize {
        let last_line = lines_count.saturating_sub(1);
        if self.window.visual_rows.is_empty() {
            let row_h = self.est_row_h.max(1.0);
            return ((doc_y.max(0.0) / row_h) as usize).min(last_line);
        }
        let rows_per_line = self.est_rows_per_line.max(f64::MIN_POSITIVE);
        let row_h = self.est_row_h.max(1.0);
        let local = doc_y - self.above_h;
        if local < 0.0 {
            let rows_above = (-local / row_h).ceil();
            let lines_above = (rows_above / rows_per_line).round() as usize;
            return self.window.start_line.saturating_sub(lines_above);
        }
        let window_h = self.window.total_height();
        if local >= window_h {
            let rows_below = (local - window_h) / row_h;
            let lines_below = (rows_below / rows_per_line).round() as usize;
            return self.window.end_line.saturating_add(lines_below).min(last_line);
        }
        let local_v = self.window.height_index.v_idx_at_y(local);
        self.window
            .visual_rows
            .get(local_v)
            .map(|v| v.line_idx)
            .unwrap_or(self.window.start_line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_stability_when_edit_is_above() {
        let text_initial = "line 0\nline 1\nline 2\nline 3\nline 4\nline 5\n";

        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut view = EditorViewState::new(tab_id, doc_id);

        // Viewport anchored at "line 3" (offset: 7*3 = 21)
        let line_3_offset = 21;
        view.viewport.offset = line_3_offset;
        view.scroll_top = 3.0 * 24.0;
        view.above_lines = 3;
        view.above_h = 3.0 * 24.0;

        // Another view inserts 100 new lines at line 0 (offset 0)
        let inserted = "inserted\n".repeat(100);
        let edit = Edit {
            start: 0,
            old_len: 0,
            new_len: inserted.len(),
        };

        view.apply_external_edit(edit, 100, 0);

        // The viewport anchor offset shifted by new_len
        assert_eq!(view.viewport.offset, line_3_offset + inserted.len());

        let text_after = format!("{}{}", inserted, text_initial);
        let line_slice = &text_after[view.viewport.offset..view.viewport.offset + 6];
        assert_eq!(line_slice, "line 3");

        // scroll_top increased by 100 lines * 24px
        assert_eq!(view.scroll_top, (3.0 + 100.0) * 24.0);
    }

    #[test]
    fn test_viewport_stability_when_edit_is_below() {
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut view = EditorViewState::new(tab_id, doc_id);

        // Viewport anchored at "line 1" (offset 7)
        let line_1_offset = 7;
        view.viewport.offset = line_1_offset;
        view.scroll_top = 24.0;

        // Edit happens at line 4 (below viewport, offset 28)
        let line_4_offset = 28;
        let inserted = "hello\nworld\n";
        let edit = Edit {
            start: line_4_offset,
            old_len: 0,
            new_len: inserted.len(),
        };

        view.apply_external_edit(edit, 2, 0);

        // Viewport anchor and scroll_top did not change at all!
        assert_eq!(view.viewport.offset, line_1_offset);
        assert_eq!(view.scroll_top, 24.0);
    }

    #[test]
    fn test_multi_window_viewport_stability_with_window_shift() {
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut view = EditorViewState::new(tab_id, doc_id);

        let start_line = 997_000;
        let end_line = 1_000_000;
        let anchor_line = 1_000_000;
        view.viewport.offset = anchor_line;
        view.cursor_offset = 50_000_000;

        let lines_vec: Vec<String> = (start_line..end_line).map(|i| format!("line {i}")).collect();
        let visual_rows: Vec<crate::markdown::VisualRow> = (start_line..end_line)
            .map(|i| crate::markdown::VisualRow {
                line_idx: i,
                sub_idx: 0,
                is_last_sub: true,
                char_start: 0,
                char_end: 10,
            })
            .collect();
        let height_index = crate::markdown::VisualRowHeightIndex::new(vec![24.0; visual_rows.len()]);

        view.window = EditorWindow {
            start_line,
            end_line,
            start_byte: 40_000_000,
            end_byte: 50_000_000,
            lines: std::sync::Arc::new(crate::editor_window::WindowLines::new(start_line, lines_vec)),
            visual_rows: std::sync::Arc::new(visual_rows),
            code_states: std::sync::Arc::new(crate::editor_window::WindowCodeStates::default()),
            height_index: std::sync::Arc::new(height_index),
            word_wrap: false,
            wrap_cols: 80,
        };

        view.above_lines = start_line;
        view.est_rows_per_line = 1.0;
        view.est_row_h = 24.0;
        view.above_h = start_line as f64 * 24.0;
        view.scroll_top = view.above_h + 100.0;

        let initial_local_scroll = view.scroll_top - view.above_h;
        assert_eq!(initial_local_scroll, 100.0);

        // View 1 at line 10 inserts 10 lines
        let edit_event = crate::state::NoteEditEvent {
            edit_line: 10,
            at: 100,
            old_len: 0,
            new_len: 10,
            line_diff: 10,
        };

        view.apply_external_edit_detailed(&edit_event);

        // 1. Invariant: local scroll (screen position of visible text) must be 100% UNCHANGED
        let new_local_scroll = view.scroll_top - view.above_h;
        assert_eq!(new_local_scroll, initial_local_scroll);

        // 2. Window lines shifted: 997,000 -> 997,010; 1,000,000 -> 1,000,010
        assert_eq!(view.window.start_line, 997_010);
        assert_eq!(view.window.end_line, 1_000_010);
        assert_eq!(view.window.lines.start(), 997_010);

        // 3. Line numbers in the visual rows incremented by 10 (gutter line numbers update!)
        assert_eq!(view.window.visual_rows.last().unwrap().line_idx, 1_000_009);

        // 4. Cursor shifted by new_len
        assert_eq!(view.cursor_offset, 50_000_010);

        // 5. Anchor line shifted by 10
        assert_eq!(view.viewport.offset, 1_000_010);

        // 6. The text inside the window is preserved exactly
        assert_eq!(view.window.line(997_010).unwrap(), "line 997000");

        // 7. Window remains non-empty and cached (zero rebuild cost on render/typing)
        assert!(!view.window.is_empty());
        assert!(!view.visual_cache_dirty);
    }
}
