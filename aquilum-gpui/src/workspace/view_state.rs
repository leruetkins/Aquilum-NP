use std::time::Instant;

use crate::document_buffer::{
    transform_offset, transform_selection_range, Bias, BufferEvent, DocumentBuffer, DocumentId,
    Edit, ViewportAnchor,
};
use crate::editor_window::EditorWindow;
use crate::state::NoteEditEvent;
use crate::workspace::session_core::ops::{EditEvent, TextOp};
use crate::workspace::session_core::undo::Selection;
use crate::workspace::TabId;

mod cache;
mod title;
mod edit;
mod visual;

#[derive(Clone, Copy, Debug)]
pub struct WheelEaseState {
    pub start: f64,
    pub target: f64,
    pub t0: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleInput {
    pub text: String,
    pub cursor_pos: usize,
    pub selection_anchor: Option<usize>,
    pub is_focused: bool,
    pub is_mouse_selecting: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseEffect {
    CommitTitle {
        tab_id: TabId,
        doc_id: DocumentId,
        new_title: String,
    },
    FlushDoc(DocumentId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseRequest {
    Ready,
    NeedsEffects(Vec<CloseEffect>),
}


#[derive(Clone, Debug)]
pub struct ViewCache {
    pub window: EditorWindow,
    pub above_lines: usize,
    pub below_lines: usize,
    pub above_h: f64,
    pub below_h: f64,
    pub est_rows_per_line: f64,
    pub est_row_h: f64,
    pub visual_cache_dirty: bool,
    /// Heading settings the current window heights were built with. Same-line
    /// edits recompute row heights against this snapshot instead of a rebuild;
    /// kept in sync by every full rebuild.
    pub headings_cache: crate::settings::HeadingsSettings,
    pub raw_table: bool,
    pub raw_tables: std::collections::HashSet<usize>,
    /// Tables (first lines) a mouse selection has already revealed as source
    /// during the ongoing drag. Revealing a table changes its row heights, so
    /// the line under a fixed pointer can then fall outside the table and the
    /// reveal decision would flip back and forth. Held until the drag ends.
    pub drag_revealed_tables: std::collections::HashSet<usize>,
    /// Image lines already reached by the current drag. Their block height is
    /// reserved either way; the hold keeps the link painted instead of
    /// flashing the picture back while the pointer moves.
    pub drag_revealed_images: std::collections::HashSet<usize>,
    pub revealed_selection: Option<(usize, usize, usize)>,
    pub cursor_line: usize,
    /// Painted state each line's rows were wrapped for. Absent means the line
    /// is in the canonical layout (markup hidden), which is what a window
    /// build produces. Only the caret line and the lines the selection touches
    /// ever enter this map, so preview and Ctrl-selection never reflow.
    pub line_paints: std::collections::HashMap<usize, crate::markdown::PaintedLineSpec>,
    /// Painted column budget of every column of each table in the window,
    /// keyed by the table's first line. The renderer owns the table layout
    /// (only it can measure text), so it publishes these budgets here and the
    /// row geometry wraps cells with exactly the same widths that are painted.
    pub table_cell_cols: std::collections::HashMap<usize, Vec<usize>>,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableCellSelection {
    pub table_first_line: usize,
    pub anchor_row: usize,
    pub anchor_col: usize,
    pub head_row: usize,
    pub head_col: usize,
}


#[derive(Clone, Debug)]
pub struct TabSession {
    pub tab_id: TabId,
    pub doc_id: DocumentId,
    pub cursor_offset: usize,
    pub selection_anchor: Option<usize>,
    pub table_cell_selection: Option<TableCellSelection>,
    pub viewport: ViewportAnchor,
    pub scroll_top: f64,
    pub wheel_ease: Option<WheelEaseState>,
    pub is_mouse_selecting: bool,
    pub word_wrap: bool,
    pub edit_mode: bool,
    pub source_mode: bool,
    pub title_input: Option<TitleInput>,
    pub measured_viewport_h: Option<f32>,
    /// Line whose markdown is fully revealed by Alt+click. Only honoured while the caret
    /// stays on that line; clicking another line or Escape clears it.
    pub reveal_all_line: Option<usize>,
    /// Image block temporarily expanded to the whole original (crop editing or
    /// "Show original"). Stored with the target name so a line move does not
    /// apply it to another picture.
    pub image_show_original: Option<(usize, String)>,
    pub cache: ViewCache,
}

pub type EditorViewState = TabSession;

impl std::ops::Deref for TabSession {
    type Target = ViewCache;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.cache
    }
}

impl std::ops::DerefMut for TabSession {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cache
    }
}

impl TabSession {
    pub fn new(tab_id: TabId, doc_id: DocumentId) -> Self {
        Self {
            tab_id,
            doc_id,
            cursor_offset: 0,
            selection_anchor: None,
            table_cell_selection: None,
            viewport: ViewportAnchor::default(),
            scroll_top: 0.0,
            wheel_ease: None,
            is_mouse_selecting: false,
            word_wrap: true,
            edit_mode: false,
            source_mode: false,
            title_input: None,
            measured_viewport_h: None,
            reveal_all_line: None,
            image_show_original: None,
            cache: ViewCache::default(),
        }
    }

    #[inline]
    pub fn title_text<'a>(&'a self, doc: &'a DocumentBuffer) -> &'a str {
        if let Some(ref ti) = self.title_input {
            &ti.text
        } else {
            &doc.title
        }
    }

    pub fn start_title_edit(&mut self, initial_text: String) {
        self.title_input = Some(TitleInput::new(initial_text));
    }

    pub fn cancel_title_edit(&mut self) {
        self.title_input = None;
    }

    pub fn shows_original_image(&self, line: usize, target: &str) -> bool {
        self.image_show_original
            .as_ref()
            .is_some_and(|(shown_line, shown_target)| *shown_line == line && shown_target == target)
    }

    pub fn set_image_show_original(&mut self, value: Option<(usize, String)>) {
        if self.image_show_original != value {
            self.image_show_original = value;
            self.cache.visual_cache_dirty = true;
        }
    }

    #[inline]
    pub fn is_table_raw(&self, first_line: usize) -> bool {
        self.raw_tables.contains(&first_line)
    }

}

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests;
