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
    /// Heading the card was opened at, when the link carried a `#fragment`.
    pub section: Option<String>,
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

struct PreviewSection {
    start_line: usize,
    /// Exclusive.
    end_line: usize,
    heading: String,
}

/// Splits a cleaned link target into its path and an optional decoded heading
/// fragment. `Note.md#Раздел` and `#Раздел` both keep their fragment.
pub(super) fn split_link_target(target: &str) -> (String, Option<String>) {
    match target.split_once('#') {
        Some((path, fragment)) => {
            let fragment = fragment.trim();
            let fragment = if fragment.is_empty() {
                None
            } else {
                Some(crate::markdown::percent_decode(fragment).trim().to_string())
            };
            (path.trim().to_string(), fragment)
        }
        None => (target.trim().to_string(), None),
    }
}

fn heading_level(kind: &crate::markdown::LineKind<'_>) -> Option<u8> {
    match kind {
        crate::markdown::LineKind::Heading1(_) => Some(1),
        crate::markdown::LineKind::Heading2(_) => Some(2),
        crate::markdown::LineKind::Heading3(_) => Some(3),
        crate::markdown::LineKind::Heading4(_) => Some(4),
        crate::markdown::LineKind::Heading5(_) => Some(5),
        crate::markdown::LineKind::Heading6(_) => Some(6),
        _ => None,
    }
}

fn heading_text<'a>(kind: &crate::markdown::LineKind<'a>) -> Option<&'a str> {
    match kind {
        crate::markdown::LineKind::Heading1(text)
        | crate::markdown::LineKind::Heading2(text)
        | crate::markdown::LineKind::Heading3(text)
        | crate::markdown::LineKind::Heading4(text)
        | crate::markdown::LineKind::Heading5(text)
        | crate::markdown::LineKind::Heading6(text) => Some(text),
        _ => None,
    }
}

fn normalize_heading(text: &str) -> String {
    crate::markdown::strip_inline_markdown(text)
        .trim()
        .trim_start_matches('#')
        .trim()
        .trim_end_matches('#')
        .trim()
        .to_lowercase()
}

fn heading_matches(text: &str, fragment: &str) -> bool {
    let heading = normalize_heading(text);
    let fragment = normalize_heading(fragment);
    if heading == fragment {
        return true;
    }
    // GitHub-style anchor: spaces become hyphens.
    let slug: String = heading
        .chars()
        .map(|ch| if ch.is_whitespace() { '-' } else { ch })
        .collect();
    slug == fragment
}

/// Finds the heading named by `fragment` and returns its section: the heading
/// line up to (but excluding) the next heading of the same or higher level.
/// Headings inside fenced code blocks do not count.
fn find_heading_section(doc: &DocumentBuffer, fragment: &str) -> Option<PreviewSection> {
    let mut scanner = crate::markdown::CodeScanner::new();
    let lines = doc.line_index.lines();
    let mut found: Option<(usize, u8, String)> = None;

    for line_idx in 0..lines {
        let line = doc.line_text(line_idx);
        if scanner.step(&line).is_some() {
            continue;
        }
        let kind = crate::markdown::classify_line(&line, None);
        let Some(level) = heading_level(&kind) else {
            continue;
        };
        let text = heading_text(&kind).unwrap_or("");

        if let Some((start_line, start_level, heading)) = found.as_ref() {
            if level <= *start_level {
                return Some(PreviewSection {
                    start_line: *start_line,
                    end_line: line_idx,
                    heading: heading.clone(),
                });
            }
        } else if heading_matches(text, fragment) {
            found = Some((
                line_idx,
                level,
                crate::markdown::strip_inline_markdown(text).trim().to_string(),
            ));
        }
    }

    found.map(|(start_line, _level, heading)| PreviewSection {
        start_line,
        end_line: lines,
        heading,
    })
}

/// The card's buffer for one section. The section text is the only copy; the
/// full note snapshot is dropped right after this.
fn section_document(doc: &DocumentBuffer, section: &PreviewSection) -> DocumentBuffer {
    let lines = doc.line_index.lines();
    let start_byte = doc.line_start_byte(section.start_line);
    let end_byte = if section.end_line < lines {
        doc.line_start_byte(section.end_line)
    } else {
        doc.len()
    };
    DocumentBuffer::new(
        DocumentId::new(),
        doc.path.clone(),
        doc.title.clone(),
        doc.slice_to_string(start_byte..end_byte),
        doc.version,
    )
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
        let (path_part, fragment) = split_link_target(&clean);
        let path = if path_part.is_empty() {
            // `#Heading` points into the note the link lives in.
            if fragment.is_none() {
                return false;
            }
            self.state
                .active_document()
                .map(|doc| doc.path.clone())
        } else {
            self.resolve_document_link_from(
                &path_part,
                self.state
                    .active_document()
                    .and_then(|doc| doc.path.parent())
                    .map(Path::to_path_buf)
                    .as_deref(),
            )
        };
        let Some(path) = path else {
            return false;
        };
        self.commit_title_rename(window, cx);
        self.close_editor_context_menu(cx);
        if !self.show_note_preview(&path, fragment.as_deref(), cx) {
            return false;
        }
        window.focus(&self.focus_handle);
        true
    }

    fn show_note_preview(
        &mut self,
        path: &Path,
        fragment: Option<&str>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(full) = self.preview_document(path) else {
            return false;
        };
        let title = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_string();
        let (doc, section) = match fragment.and_then(|fragment| find_heading_section(&full, fragment))
        {
            Some(section) => {
                let doc = section_document(&full, &section);
                (doc, Some(section.heading))
            }
            None => (full, None),
        };
        let view = EditorViewState::new(TabId::new(), doc.id);
        self.note_preview = Some(NotePreviewState {
            path: path.to_path_buf(),
            title,
            section,
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

    /// Whether the internal note behind `raw_url` exists on disk. External
    /// links and same-note anchors count as existing. The card resolves
    /// relative to its own note; the editor resolves relative to the active
    /// note.
    pub(crate) fn link_target_exists(&self, raw_url: &str) -> bool {
        let clean = crate::markdown::clean_link_url(raw_url);
        if clean.is_empty() || is_external(&clean) {
            return true;
        }
        let (path_part, _fragment) = split_link_target(&clean);
        if path_part.is_empty() {
            return true;
        }
        let base_dir = match self.note_preview.as_ref() {
            Some(state) => state.path.parent().map(Path::to_path_buf),
            None => self
                .state
                .active_document()
                .and_then(|doc| doc.path.parent())
                .map(Path::to_path_buf),
        };
        self.resolve_document_link_from(&path_part, base_dir.as_deref())
            .map_or(false, |path| path.is_file())
    }

    /// Line of the heading named by a link fragment, for cursor placement in
    /// the editor.
    pub(crate) fn heading_line_for_fragment(
        doc: &DocumentBuffer,
        fragment: &str,
    ) -> Option<usize> {
        find_heading_section(doc, fragment).map(|section| section.start_line)
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
        let (path_part, fragment) = split_link_target(&clean);
        let (base_dir, current_path) = match self.note_preview.as_ref() {
            Some(state) => (
                state.path.parent().map(Path::to_path_buf),
                Some(state.path.clone()),
            ),
            None => (None, None),
        };
        let path = if path_part.is_empty() {
            // `#Heading` points into the card's own note.
            if fragment.is_none() {
                return;
            }
            current_path
        } else {
            self.resolve_document_link_from(&path_part, base_dir.as_deref())
        };
        let Some(path) = path else {
            return;
        };
        self.show_note_preview(&path, fragment.as_deref(), cx);
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

#[cfg(test)]
mod tests;
