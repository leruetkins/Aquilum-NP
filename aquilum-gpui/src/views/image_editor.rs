//! In-place image editor: non-destructive crop frame, "show original" mode,
//! and the vector annotation editor. The editor state lives in `AppView`; this
//! module owns its data, actions and floating panels.

use std::path::PathBuf;

use gpui::{div, prelude::*, px, Context, Div, IntoElement, MouseButton, MouseDownEvent, Point as GPoint, SharedString, Window};

use crate::icons;
use crate::images::{self, Annotation, AnnotationKind, CropRect, ImageEdit, Point, RectF};
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::notification::NotificationType;
use crate::views::text_input::TextInput;

mod caption;
mod crop;
mod editor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageEditorMode {
    Crop,
    Annotate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CropAspect {
    Free,
    Original,
    Ratio(u32, u32),
}

impl CropAspect {
    pub fn ratio(self, natural: (u32, u32)) -> Option<f32> {
        match self {
            Self::Free => None,
            Self::Original => (natural.1 > 0).then(|| natural.0 as f32 / natural.1 as f32),
            Self::Ratio(w, h) => (h > 0).then(|| w as f32 / h as f32),
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Free => "Свободно".to_string(),
            Self::Original => "Как у оригинала".to_string(),
            Self::Ratio(w, h) => format!("{w}:{h}"),
        }
    }

    pub fn swapped(self) -> Self {
        match self {
            Self::Ratio(w, h) => Self::Ratio(h, w),
            other => other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CropHandle {
    Nw,
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CropDragKind {
    Move,
    Handle(CropHandle),
}

#[derive(Debug, Clone)]
pub struct CropDrag {
    pub kind: CropDragKind,
    pub start: (f32, f32),
    pub frame: CropRect,
    pub ratio: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationTool {
    Select,
    Arrow,
    Line,
    Rect,
    Ellipse,
    Highlight,
    Pencil,
    Text,
    Label,
    Blur,
    Pixelate,
}

impl AnnotationTool {
    pub fn is_draw(self) -> bool {
        matches!(
            self,
            Self::Arrow
                | Self::Line
                | Self::Rect
                | Self::Ellipse
                | Self::Highlight
                | Self::Pencil
                | Self::Blur
                | Self::Pixelate
        )
    }

    pub fn kind(self) -> AnnotationKind {
        match self {
            Self::Arrow => AnnotationKind::Arrow,
            Self::Line => AnnotationKind::Line,
            Self::Rect => AnnotationKind::Rect,
            Self::Ellipse => AnnotationKind::Ellipse,
            Self::Highlight => AnnotationKind::Highlight,
            Self::Pencil => AnnotationKind::Pencil,
            Self::Text => AnnotationKind::Text,
            Self::Label => AnnotationKind::Label,
            Self::Select | Self::Blur | Self::Pixelate => AnnotationKind::Arrow,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnnotationDrag {
    /// Original annotation points when the drag started (select/handle move),
    /// or the single point of a fresh draft.
    pub origin: Vec<Point>,
    pub start_image: Point,
    pub index: Option<usize>,
    /// `Some(point_index)` for a resize handle, `None` for move/draw.
    pub handle: Option<usize>,
    pub draft: Option<Annotation>,
}

pub struct TextEntry {
    pub input: TextInput,
    pub point: Point,
    pub focus: gpui::FocusHandle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfirmAction {
    ApplyCrop,
    Flatten,
    Blur(RectF),
    Pixelate(RectF),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CropField {
    X,
    Y,
    W,
    H,
}

pub struct ImageEditorState {
    pub mode: ImageEditorMode,
    pub line: usize,
    pub target: String,
    pub path: PathBuf,
    pub natural: (u32, u32),
    pub edit: ImageEdit,
    pub saved_edit: ImageEdit,
    pub frame: CropRect,
    pub aspect: CropAspect,
    pub drag: Option<CropDrag>,
    pub crop_fields: Vec<TextInput>,
    pub active_field: Option<usize>,
    pub ratio_fields: Vec<TextInput>,
    pub active_ratio: Option<usize>,
    pub tool: AnnotationTool,
    pub color: String,
    pub width: f32,
    pub filled: bool,
    pub selected: Option<usize>,
    pub undo: Vec<Vec<Annotation>>,
    pub redo: Vec<Vec<Annotation>>,
    pub annotation_drag: Option<AnnotationDrag>,
    pub text_entry: Option<TextEntry>,
    pub confirm: Option<ConfirmAction>,
    /// Last painted image rectangle in window coordinates: x, y, w, h.
    /// Interior mutability lets the render pass publish it without `&mut self`.
    pub rect: std::cell::Cell<(f32, f32, f32, f32)>,
    pub scale: std::cell::Cell<f32>,
}

const PALETTE: &[&str] = &[
    "#e5484d", "#f76b15", "#ffb224", "#30a46c", "#0091ff", "#6e56cf", "#e93d82", "#121212",
];
const MIN_CROP: f32 = 8.0;
const PANEL_W: f32 = 276.0;


fn drop_image_asset(path: &std::path::Path, cx: &mut gpui::App) {
    gpui::ImageSource::from(path).remove_asset(cx);
}

impl ImageEditorState {
    fn sync_crop_fields_except(&mut self, skip: Option<usize>) {
        let values = [
            self.frame.x.round() as i64,
            self.frame.y.round() as i64,
            self.frame.w.round() as i64,
            self.frame.h.round() as i64,
        ];
        for (index, (input, value)) in self.crop_fields.iter_mut().zip(values).enumerate() {
            if skip == Some(index) {
                continue;
            }
            input.set_text(value.to_string());
        }
        if let CropAspect::Ratio(w, h) = self.aspect {
            if skip != Some(4) {
                self.ratio_fields[0].set_text(w.to_string());
            }
            if skip != Some(5) {
                self.ratio_fields[1].set_text(h.to_string());
            }
        }
    }

    fn sync_crop_fields(&mut self) {
        let values = [
            self.frame.x.round() as i64,
            self.frame.y.round() as i64,
            self.frame.w.round() as i64,
            self.frame.h.round() as i64,
        ];
        for (input, value) in self.crop_fields.iter_mut().zip(values) {
            input.set_text(value.to_string());
        }
        if let CropAspect::Ratio(w, h) = self.aspect {
            self.ratio_fields[0].set_text(w.to_string());
            self.ratio_fields[1].set_text(h.to_string());
        }
    }

    pub fn image_edit_for_display(&self) -> ImageEdit {
        self.edit.clone()
    }
}

fn annotation_label(kind: AnnotationKind, lang: &str) -> String {
    let key = match kind {
        AnnotationKind::Arrow => "editor.image.kind_arrow",
        AnnotationKind::Line => "editor.image.kind_line",
        AnnotationKind::Rect => "editor.image.kind_rect",
        AnnotationKind::Ellipse => "editor.image.kind_ellipse",
        AnnotationKind::Highlight => "editor.image.kind_highlight",
        AnnotationKind::Pencil => "editor.image.kind_pencil",
        AnnotationKind::Text => "editor.image.kind_text",
        AnnotationKind::Label => "editor.image.kind_label",
    };
    crate::i18n::t(key, lang)
}


// ───────────────────────────── Caption editor ─────────────────────────────

pub struct CaptionEditorState {
    pub line: usize,
    pub path: PathBuf,
    pub edit: ImageEdit,
    pub input: TextInput,
    pub focus: gpui::FocusHandle,
    pub position: String,
    pub numbered: bool,
    pub position_menu_open: bool,
}

