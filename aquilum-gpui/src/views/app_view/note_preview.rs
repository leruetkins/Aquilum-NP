//! Ctrl+Shift link card: the linked note rendered read-only with the same
//! per-row markdown renderer the editor uses, including mouse selection and
//! clipboard copy.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{div, prelude::*, px, Context, IntoElement, MouseButton, ScrollWheelEvent, Window};

use super::{
    render_live_styled_row, shape_row, shape_table_row, AppView, Theme, EDITOR_OVERSCAN_PX,
};
use crate::document_buffer::{DocumentBuffer, DocumentId};
use crate::markdown::{InlineReveal, VisualRow};
use crate::workspace::{EditorViewState, TabId};

const PREVIEW_PAD: f32 = 18.0;
const PREVIEW_HEADER_H: f32 = 48.0;
const PREVIEW_SCROLLBAR_LANE: f32 = 14.0;

pub struct NotePreviewState {
    pub path: PathBuf,
    pub title: String,
    pub doc: DocumentBuffer,
    pub view: EditorViewState,
    pub scroll_top: f64,
    table_layouts: RefCell<HashMap<usize, (u64, Arc<super::TableLayout>)>>,
    pub is_mouse_selecting: bool,
    pub drag_started: bool,
    pub mouse_down: Option<(f32, f32)>,
}

struct PreviewLayout {
    panel_w: f32,
    panel_h: f32,
    body_top: f32,
    body_h: f32,
    text_left: f32,
    content_w: f32,
}

fn preview_layout(window: &Window) -> PreviewLayout {
    let window_w: f32 = window.bounds().size.width.into();
    let window_h: f32 = window.bounds().size.height.into();
    let panel_w = (760.0f32).min(window_w - 64.0).max(320.0);
    let panel_h = (window_h - 96.0).min(680.0).max(240.0);
    let panel_left = (window_w - panel_w) / 2.0;
    let panel_top = (window_h - panel_h) / 2.0;
    let body_h = (panel_h - PREVIEW_HEADER_H).max(1.0);
    PreviewLayout {
        panel_w,
        panel_h,
        body_top: panel_top + PREVIEW_HEADER_H,
        body_h,
        text_left: panel_left + PREVIEW_PAD,
        content_w: (panel_w - 2.0 * PREVIEW_PAD - PREVIEW_SCROLLBAR_LANE).max(1.0),
    }
}

fn is_external(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:")
}

impl AppView {
    /// Opens the note behind `raw_url` in the preview dialog. Returns `false`
    /// for web links and unreadable targets, so the caller keeps its normal
    /// click behavior.
    pub(crate) fn open_note_preview(
        &mut self,
        raw_url: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let clean = crate::markdown::clean_link_url(raw_url);
        if clean.is_empty() || is_external(&clean) {
            return false;
        }
        let Some(path) = self.resolve_document_link(&clean) else {
            return false;
        };
        self.commit_title_rename(window, cx);
        self.close_editor_context_menu(cx);
        if !self.show_note_preview(&path, cx) {
            return false;
        }
        window.focus(&self.focus_handle);
        true
    }

    fn show_note_preview(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.preview_document(path) else {
            return false;
        };
        let title = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_string();
        let view = EditorViewState::new(TabId::new(), doc.id);
        self.note_preview = Some(NotePreviewState {
            path: path.to_path_buf(),
            title,
            doc,
            view,
            scroll_top: 0.0,
            table_layouts: RefCell::new(HashMap::new()),
            is_mouse_selecting: false,
            drag_started: false,
            mouse_down: None,
        });
        self.note_preview_scrollbar_drag.end();
        cx.notify();
        true
    }

    /// A snapshot buffer for the card. An already loaded note contributes its
    /// rope, which clones structurally, so its text is not copied again.
    fn preview_document(&self, path: &Path) -> Option<DocumentBuffer> {
        let id = DocumentId::new();
        let title = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_string();
        if let Some(doc_id) = self.state.find_document_by_path(path) {
            if self.state.is_document_loaded(doc_id) {
                if let Some(source) = self.state.documents.get(&doc_id) {
                    return Some(DocumentBuffer::from_rope(
                        id,
                        path.to_path_buf(),
                        title,
                        source.rope().clone(),
                        source.version,
                    ));
                }
            }
        }
        DocumentBuffer::from_file(id, path.to_path_buf(), title, 1).ok()
    }

    /// Follows a markdown link inside the card: another note replaces the
    /// card, a web link opens externally.
    pub(crate) fn handle_note_preview_link(&mut self, raw_url: &str, cx: &mut Context<Self>) {
        let clean = crate::markdown::clean_link_url(raw_url);
        if clean.is_empty() {
            return;
        }
        if is_external(&clean) {
            cx.open_url(&clean);
            return;
        }
        let base_dir = self
            .note_preview
            .as_ref()
            .and_then(|state| state.path.parent())
            .map(Path::to_path_buf);
        let Some(path) = self.resolve_document_link_from(&clean, base_dir.as_deref()) else {
            return;
        };
        self.show_note_preview(&path, cx);
    }

    pub fn close_note_preview(&mut self, cx: &mut Context<Self>) {
        if self.note_preview.take().is_some() {
            self.note_preview_scrollbar_drag.end();
            cx.notify();
        }
    }

    pub(crate) fn handle_note_preview_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ctrl = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
        let key = event.keystroke.key.to_lowercase();
        if key == "escape" {
            self.close_note_preview(cx);
            window.focus(&self.focus_handle);
            return;
        }
        if ctrl && (key == "c" || key == "с") {
            if let Some(state) = self.note_preview.as_ref() {
                if let Some(text) = state.view.selected_text(&state.doc) {
                    if !text.is_empty() {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                    }
                }
            }
            return;
        }
        if ctrl && (key == "a" || key == "ф") {
            if let Some(state) = self.note_preview.as_mut() {
                state.view.select_all(&state.doc);
                cx.notify();
            }
        }
    }

    fn note_preview_max_scroll(&self, body_h: f32) -> f64 {
        let Some(state) = self.note_preview.as_ref() else {
            return 0.0;
        };
        Self::editor_max_scroll(state.view.total_height(), f64::from(body_h))
    }

    /// Rebuilds the visual cache for the card's panel width and clamps the
    /// scroll to the freshly measured content.
    pub(super) fn prepare_note_preview(&mut self, window: &Window, cx: &gpui::App) {
        let layout = preview_layout(window);
        let typography = self.editor_typography();
        let wrap_cols = {
            let char_w = self.editor_char_width(cx).max(1.0);
            ((layout.content_w / char_w).floor() as usize).max(8)
        };
        let Some(state) = self.note_preview.as_mut() else {
            return;
        };
        let anchor_line = state
            .view
            .line_at_doc_y(state.doc.line_index.lines(), state.scroll_top);
        state.view.viewport.offset = anchor_line;
        state.view.ensure_visual_cache_with_headings(
            &mut state.doc,
            true,
            wrap_cols,
            Some(&typography),
            anchor_line,
            true,
        );
        let max = Self::editor_max_scroll(state.view.total_height(), f64::from(layout.body_h));
        state.scroll_top = state.scroll_top.clamp(0.0, max);
    }

    pub(super) fn handle_note_preview_scrollbar_drag(
        &mut self,
        pointer_y: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.note_preview_scrollbar_drag.active {
            return;
        }
        let layout = preview_layout(window);
        let max = self.note_preview_max_scroll(layout.body_h) as f32;
        let drag = self.note_preview_scrollbar_drag;
        let Some(state) = self.note_preview.as_mut() else {
            return;
        };
        let style = crate::views::scrollbar::ScrollbarStyle::default();
        let Some(geometry) = crate::views::scrollbar::ScrollbarGeometry::new(
            layout.body_top,
            layout.body_h,
            max,
            state.scroll_top as f32,
            &style,
        ) else {
            return;
        };
        let target = geometry
            .offset_after_drag(&drag, pointer_y)
            .clamp(0.0, max);
        state.scroll_top = f64::from(target);
        cx.notify();
    }

    fn set_note_preview_scroll(
        &mut self,
        scroll: f64,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let layout = preview_layout(window);
        let max = self.note_preview_max_scroll(layout.body_h);
        if let Some(state) = self.note_preview.as_mut() {
            state.scroll_top = scroll.clamp(0.0, max);
            cx.notify();
        }
    }

    fn handle_note_preview_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let layout = preview_layout(window);
        let row_h = f64::from(self.editor_typography().row_height);
        let Some(state) = self.note_preview.as_mut() else {
            return;
        };
        let delta = match event.delta {
            gpui::ScrollDelta::Pixels(p) => f64::from(f32::from(p.y)),
            gpui::ScrollDelta::Lines(l) => f64::from(l.y) * row_h,
        };
        if delta == 0.0 {
            return;
        }
        let max = Self::editor_max_scroll(state.view.total_height(), f64::from(layout.body_h));
        let next = (state.scroll_top - delta).clamp(0.0, max);
        if next != state.scroll_top {
            state.scroll_top = next;
            cx.notify();
        }
    }

}

mod interact;
mod render;
