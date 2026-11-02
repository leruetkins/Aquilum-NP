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

impl AppView {
    pub fn open_image_editor(
        &mut self,
        mode: ImageEditorMode,
        line: usize,
        image: &images::ResolvedImage,
        cx: &mut Context<Self>,
    ) {
        if self.image_editor.is_some() {
            return;
        }
        let crop_fields = vec![
            TextInput::new(cx.focus_handle()),
            TextInput::new(cx.focus_handle()),
            TextInput::new(cx.focus_handle()),
            TextInput::new(cx.focus_handle()),
        ];
        let ratio_fields = vec![TextInput::new(cx.focus_handle()), TextInput::new(cx.focus_handle())];
        let frame = image
            .edit
            .crop
            .unwrap_or_else(|| CropRect::full(image.natural))
            .clamp_to(image.natural);
        let mut editor = ImageEditorState {
            mode,
            line,
            target: image.reference.target.clone(),
            path: image.path.clone(),
            natural: image.natural,
            edit: image.edit.clone(),
            saved_edit: image.edit.clone(),
            frame,
            aspect: CropAspect::Free,
            drag: None,
            crop_fields,
            active_field: None,
            ratio_fields,
            active_ratio: None,
            tool: AnnotationTool::Arrow,
            color: PALETTE[0].to_string(),
            width: 3.0,
            filled: false,
            selected: None,
            undo: Vec::new(),
            redo: Vec::new(),
            annotation_drag: None,
            text_entry: None,
            confirm: None,
            rect: std::cell::Cell::new(
                self.image_editor_rect.get().unwrap_or((0.0, 0.0, 1.0, 1.0)),
            ),
            scale: std::cell::Cell::new(1.0),
        };
        editor.sync_crop_fields();
        self.image_editor = Some(editor);
        if let Some((view, _)) = self.state.active_view_and_doc_mut() {
            view.set_image_show_original(Some((line, image.reference.target.clone())));
            view.visual_cache_dirty = true;
        }
        self.select_image_line(line);
        self.hovered_image_line = Some(line);
        cx.notify();
    }

    pub fn close_image_editor(&mut self, save: bool, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.take() else {
            return;
        };
        let edit = if save { editor.edit.clone() } else { editor.saved_edit.clone() };
        let _ = images::save_edit(&editor.path, &edit);
        if let Some((view, _)) = self.state.active_view_and_doc_mut() {
            view.image_show_original = None;
            view.visual_cache_dirty = true;
        }
        images::invalidate_resolved_images();
        crate::image_render::invalidate_decoded();
        self.clear_image_selection();
        cx.notify();
    }

    fn sync_crop_frame(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        let frame = editor.frame.clamp_to(editor.natural);
        editor.frame = frame;
        let natural = editor.natural;
        editor.edit.crop = if (frame.w >= natural.0 as f32 - 0.5)
            && (frame.h >= natural.1 as f32 - 0.5)
            && frame.x <= 0.5
            && frame.y <= 0.5
        {
            None
        } else {
            Some(frame)
        };
        editor.sync_crop_fields();
        cx.notify();
    }

    pub fn image_editor_drag_start(
        &mut self,
        kind: CropDragKind,
        position: GPoint<gpui::Pixels>,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        let mut ratio = editor.aspect.ratio(editor.natural);
        if shift {
            ratio = if ratio.is_some() {
                None
            } else if editor.frame.h > 0.0 {
                Some(editor.frame.w / editor.frame.h)
            } else {
                None
            };
        }
        editor.drag = Some(CropDrag {
            kind,
            start: (f32::from(position.x), f32::from(position.y)),
            frame: editor.frame,
            ratio,
        });
        cx.notify();
    }

    pub fn image_editor_drag_move(
        &mut self,
        position: GPoint<gpui::Pixels>,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        let Some(drag) = editor.drag.clone() else {
            return;
        };
        let scale = editor.scale.get().max(0.0001);
        let dx = (f32::from(position.x) - drag.start.0) / scale;
        let dy = (f32::from(position.y) - drag.start.1) / scale;
        let natural = editor.natural;
        let mut frame = drag.frame;
        let ratio = drag.ratio;
        match drag.kind {
            CropDragKind::Move => {
                frame.x = (drag.frame.x + dx).clamp(0.0, (natural.0 as f32 - frame.w).max(0.0));
                frame.y = (drag.frame.y + dy).clamp(0.0, (natural.1 as f32 - frame.h).max(0.0));
            }
            CropDragKind::Handle(handle) => {
                let mut left = drag.frame.x;
                let mut top = drag.frame.y;
                let mut right = drag.frame.x + drag.frame.w;
                let mut bottom = drag.frame.y + drag.frame.h;
                let horizontal = matches!(handle, CropHandle::Nw | CropHandle::W | CropHandle::Sw);
                let vertical = matches!(handle, CropHandle::Nw | CropHandle::N | CropHandle::Ne);
                let right_edge = matches!(handle, CropHandle::Ne | CropHandle::E | CropHandle::Se);
                let bottom_edge = matches!(handle, CropHandle::Se | CropHandle::S | CropHandle::Sw);
                if horizontal {
                    left = (drag.frame.x + dx).min(right - MIN_CROP);
                }
                if right_edge {
                    right = (right + dx).max(left + MIN_CROP);
                }
                if vertical {
                    top = (drag.frame.y + dy).min(bottom - MIN_CROP);
                }
                if bottom_edge {
                    bottom = (bottom + dy).max(top + MIN_CROP);
                }
                left = left.max(0.0);
                top = top.max(0.0);
                right = right.min(natural.0 as f32);
                bottom = bottom.min(natural.1 as f32);
                frame = CropRect {
                    x: left,
                    y: top,
                    w: (right - left).max(MIN_CROP),
                    h: (bottom - top).max(MIN_CROP),
                };
                if let Some(ratio) = ratio {
                    let cx0 = drag.frame.x + drag.frame.w / 2.0;
                    let cy0 = drag.frame.y + drag.frame.h / 2.0;
                    if right_edge || horizontal {
                        let w = frame.w;
                        let h = w / ratio;
                        let anchor_bottom = bottom_edge;
                        frame.h = h.min(natural.1 as f32);
                        frame.w = (frame.h * ratio).min(natural.0 as f32);
                        frame.y = if anchor_bottom { bottom - frame.h } else { cy0 - frame.h / 2.0 };
                        frame.x = if horizontal { right - frame.w } else { cx0 - frame.w / 2.0 };
                    } else {
                        let h = frame.h;
                        let w = h * ratio;
                        frame.w = w.min(natural.0 as f32);
                        frame.h = (frame.w / ratio).min(natural.1 as f32);
                        frame.y = if vertical { bottom - frame.h } else { cy0 - frame.h / 2.0 };
                        frame.x = cx0 - frame.w / 2.0;
                    }
                    frame.x = frame.x.clamp(0.0, (natural.0 as f32 - frame.w).max(0.0));
                    frame.y = frame.y.clamp(0.0, (natural.1 as f32 - frame.h).max(0.0));
                }
            }
        }
        editor.frame = frame.clamp_to(natural);
        let _ = shift;
        cx.notify();
    }

    pub fn image_editor_drag_end(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        if editor.drag.take().is_some() {
            let frame = editor.frame;
            editor.frame = frame;
        }
        self.sync_crop_frame(cx);
    }

    pub fn image_editor_set_aspect(&mut self, aspect: CropAspect, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        editor.aspect = aspect;
        if let Some(ratio) = aspect.ratio(editor.natural) {
            let natural = editor.natural;
            let center_x = editor.frame.x + editor.frame.w / 2.0;
            let center_y = editor.frame.y + editor.frame.h / 2.0;
            let mut w = editor.frame.w;
            let mut h = w / ratio;
            if h > natural.1 as f32 {
                h = natural.1 as f32;
                w = h * ratio;
            }
            if w > natural.0 as f32 {
                w = natural.0 as f32;
                h = w / ratio;
            }
            editor.frame = CropRect {
                x: (center_x - w / 2.0).clamp(0.0, (natural.0 as f32 - w).max(0.0)),
                y: (center_y - h / 2.0).clamp(0.0, (natural.1 as f32 - h).max(0.0)),
                w,
                h,
            };
        }
        self.sync_crop_frame(cx);
    }

    pub fn image_editor_swap_aspect(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        editor.aspect = editor.aspect.swapped();
        let aspect = editor.aspect;
        self.image_editor_set_aspect(aspect, cx);
    }

    pub fn image_editor_reset_crop(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        editor.frame = CropRect::full(editor.natural);
        editor.aspect = CropAspect::Free;
        self.sync_crop_frame(cx);
    }

    pub fn image_editor_set_field(&mut self, field: CropField, value: f32, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        let mut frame = editor.frame;
        match field {
            CropField::X => frame.x = value,
            CropField::Y => frame.y = value,
            CropField::W => frame.w = value.max(MIN_CROP),
            CropField::H => frame.h = value.max(MIN_CROP),
        }
        if let Some(ratio) = editor.aspect.ratio(editor.natural) {
            match field {
                CropField::W => frame.h = (frame.w / ratio).max(MIN_CROP),
                CropField::H => frame.w = (frame.h * ratio).max(MIN_CROP),
                _ => {}
            }
        }
        let frame = frame.clamp_to(editor.natural);
        editor.frame = frame;
        let natural = editor.natural;
        editor.edit.crop = if (frame.w >= natural.0 as f32 - 0.5)
            && (frame.h >= natural.1 as f32 - 0.5)
            && frame.x <= 0.5
            && frame.y <= 0.5
        {
            None
        } else {
            Some(frame)
        };
        let skip = match field {
            CropField::X => Some(0),
            CropField::Y => Some(1),
            CropField::W => Some(2),
            CropField::H => Some(3),
        };
        editor.sync_crop_fields_except(skip);
        cx.notify();
    }

    pub fn image_editor_apply_crop(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        let path = editor.path.clone();
        let mut edit = editor.edit.clone();
        if crate::image_render::apply_crop_file(&path, &mut edit).is_ok() {
            editor.edit = edit.clone();
            editor.saved_edit = edit;
            editor.natural = crate::images::image_size(&path).unwrap_or(editor.natural);
            editor.frame = CropRect::full(editor.natural);
            editor.aspect = CropAspect::Free;
            editor.sync_crop_fields();
            images::invalidate_resolved_images();
            drop_image_asset(&path, cx);
            if let Some((view, _)) = self.state.active_view_and_doc_mut() {
                view.visual_cache_dirty = true;
            }
            cx.notify();
        } else {
            self.push_notification(
                NotificationType::Error,
                None,
                crate::i18n::t("editor.image.crop_failed", &self.config.ui.language),
                Some(5000),
                cx,
            );
        }
    }

    pub fn image_editor_flatten(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        let path = editor.path.clone();
        let mut edit = editor.edit.clone();
        if crate::image_render::flatten_file(&path, &mut edit).is_ok() {
            editor.edit = edit.clone();
            editor.saved_edit = edit;
            images::invalidate_resolved_images();
            drop_image_asset(&path, cx);
            cx.notify();
        }
    }

    pub fn image_editor_blur(&mut self, region: RectF, pixelate: bool, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        let path = editor.path.clone();
        if crate::image_render::blur_region_file(&path, region, pixelate).is_ok() {
            images::invalidate_resolved_images();
            drop_image_asset(&path, cx);
            cx.notify();
        }
    }

    // ─────────────────────── Annotations ───────────────────────

    fn push_undo(&mut self) {
        if let Some(editor) = self.image_editor.as_mut() {
            editor.undo.push(editor.edit.annotations.clone());
            if editor.undo.len() > 100 {
                editor.undo.remove(0);
            }
            editor.redo.clear();
        }
    }

    pub fn image_editor_undo(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        if let Some(previous) = editor.undo.pop() {
            editor.redo.push(editor.edit.annotations.clone());
            editor.edit.annotations = previous;
            editor.selected = None;
            cx.notify();
        }
    }

    pub fn image_editor_redo(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        if let Some(next) = editor.redo.pop() {
            editor.undo.push(editor.edit.annotations.clone());
            editor.edit.annotations = next;
            editor.selected = None;
            cx.notify();
        }
    }

    pub fn image_editor_delete_selected(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.image_editor.as_ref().and_then(|editor| editor.selected) else {
            return;
        };
        self.push_undo();
        if let Some(editor) = self.image_editor.as_mut() {
            editor.selected = None;
            if index < editor.edit.annotations.len() {
                editor.edit.annotations.remove(index);
            }
        }
        cx.notify();
    }

    pub fn image_editor_move_layer(&mut self, index: usize, delta: isize, cx: &mut Context<Self>) {
        self.push_undo();
        if let Some(editor) = self.image_editor.as_mut() {
            let len = editor.edit.annotations.len();
            if index >= len {
                return;
            }
            let target = (index as isize + delta).clamp(0, len as isize - 1) as usize;
            if target != index {
                editor.edit.annotations.swap(index, target);
                editor.selected = Some(target);
            }
        }
        cx.notify();
    }

    pub fn image_editor_pointer_down(
        &mut self,
        position: GPoint<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        let (rx, ry, _, _) = editor.rect.get();
        let scale = editor.scale.get().max(0.0001);
        let point = Point::new(
            (f32::from(position.x) - rx) / scale,
            (f32::from(position.y) - ry) / scale,
        );
        match editor.tool {
            AnnotationTool::Select => {
                let hit = editor
                    .edit
                    .annotations
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, annotation)| annotation.hit(point, 10.0 / scale))
                    .map(|(index, _)| index);
                editor.selected = hit;
                if let Some(index) = hit {
                    let origin = editor.edit.annotations[index].points.clone();
                    editor.annotation_drag = Some(AnnotationDrag {
                        origin,
                        start_image: point,
                        index: Some(index),
                        handle: None,
                        draft: None,
                    });
                }
                cx.notify();
            }
            AnnotationTool::Text | AnnotationTool::Label => {
                let focus = cx.focus_handle();
                let mut input = TextInput::new(focus.clone());
                input.set_text("");
                window.focus(&focus);
                editor.text_entry = Some(TextEntry {
                    input,
                    point,
                    focus,
                });
                cx.notify();
            }
            tool if tool.is_draw() => {
                let mut annotation = Annotation::with_points(tool.kind(), vec![point, point]);
                annotation.color = editor.color.clone();
                annotation.width = editor.width;
                annotation.filled = editor.filled;
                editor.annotation_drag = Some(AnnotationDrag {
                    origin: vec![point],
                    start_image: point,
                    index: None,
                    handle: None,
                    draft: Some(annotation),
                });
                cx.notify();
            }
            _ => {}
        }
    }

    pub fn image_editor_pointer_move(
        &mut self,
        position: GPoint<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        let Some(drag) = editor.annotation_drag.as_mut() else {
            return;
        };
        let (rx, ry, _, _) = editor.rect.get();
        let scale = editor.scale.get().max(0.0001);
        let point = Point::new(
            (f32::from(position.x) - rx) / scale,
            (f32::from(position.y) - ry) / scale,
        );
        if let Some(draft) = drag.draft.as_mut() {
            match draft.kind {
                AnnotationKind::Pencil => {
                    if let Some(last) = draft.points.last() {
                        if (last.x - point.x).abs() > 1.5 || (last.y - point.y).abs() > 1.5 {
                            draft.points.push(point);
                        }
                    }
                }
                AnnotationKind::Highlight => {
                    if let Some(last) = draft.points.last() {
                        if (last.x - point.x).abs() > 2.0 || (last.y - point.y).abs() > 2.0 {
                            draft.points.push(point);
                        }
                    }
                }
                _ => {
                    if draft.points.len() >= 2 {
                        draft.points[1] = point;
                    } else {
                        draft.points.push(point);
                    }
                }
            }
        } else if let Some(index) = drag.index {
            let dx = point.x - drag.start_image.x;
            let dy = point.y - drag.start_image.y;
            if let Some(annotation) = editor.edit.annotations.get_mut(index) {
                if let Some(handle) = drag.handle {
                    if let Some(target) = annotation.points.get_mut(handle) {
                        *target = point;
                    }
                } else {
                    annotation.points = drag
                        .origin
                        .iter()
                        .map(|p| Point::new(p.x + dx, p.y + dy))
                        .collect();
                }
            }
        }
        cx.notify();
    }

    pub fn image_editor_pointer_up(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self
            .image_editor
            .as_mut()
            .and_then(|editor| editor.annotation_drag.take())
        else {
            return;
        };
        let tool = self
            .image_editor
            .as_ref()
            .map(|editor| editor.tool)
            .unwrap_or(AnnotationTool::Select);
        if let Some(draft) = drag.draft {
            let region = draft.bounds().unwrap_or(RectF::default());
            match tool {
                AnnotationTool::Blur | AnnotationTool::Pixelate => {
                    if let Some(editor) = self.image_editor.as_mut() {
                        editor.confirm = Some(if matches!(tool, AnnotationTool::Blur) {
                            ConfirmAction::Blur(region)
                        } else {
                            ConfirmAction::Pixelate(region)
                        });
                    }
                }
                _ => {
                    if !preview_is_tiny(&draft) && !draft.points.is_empty() {
                        self.push_undo();
                        if let Some(editor) = self.image_editor.as_mut() {
                            let mut committed = draft;
                            if matches!(committed.kind, AnnotationKind::Label) {
                                let labels = editor
                                    .edit
                                    .annotations
                                    .iter()
                                    .filter(|a| matches!(a.kind, AnnotationKind::Label))
                                    .count()
                                    + 1;
                                committed.text = Some(labels.to_string());
                            }
                            editor.edit.annotations.push(committed);
                            editor.selected = Some(editor.edit.annotations.len() - 1);
                        }
                    }
                }
            }
        } else if let Some(index) = drag.index {
            if let Some(editor) = self.image_editor.as_mut() {
                if let Some(annotation) = editor.edit.annotations.get_mut(index) {
                    if drag.handle.is_some()
                        && matches!(annotation.kind, AnnotationKind::Rect | AnnotationKind::Ellipse)
                    {
                        if let (Some(first), Some(last)) =
                            (annotation.points.first().copied(), annotation.points.last().copied())
                        {
                            annotation.points = vec![first, last];
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    pub fn image_editor_commit_text(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        let Some(entry) = editor.text_entry.take() else {
            return;
        };
        let text = entry.input.text().trim().to_string();
        if text.is_empty() {
            cx.notify();
            return;
        }
        let mut annotation = Annotation {
            kind: if matches!(editor.tool, AnnotationTool::Label) {
                AnnotationKind::Label
            } else {
                AnnotationKind::Text
            },
            points: vec![entry.point],
            color: editor.color.clone(),
            width: editor.width,
            filled: false,
            text: Some(text),
            font_size: (editor.width * 5.0).clamp(12.0, 48.0),
        };
        if matches!(annotation.kind, AnnotationKind::Label) {
            let labels = editor
                .edit
                .annotations
                .iter()
                .filter(|a| matches!(a.kind, AnnotationKind::Label))
                .count()
                + 1;
            annotation.text = Some(labels.to_string());
        }
        editor.undo.push(editor.edit.annotations.clone());
        editor.redo.clear();
        editor.edit.annotations.push(annotation);
        editor.selected = Some(editor.edit.annotations.len() - 1);
        cx.notify();
    }

    pub fn image_editor_select_layer(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(editor) = self.image_editor.as_mut() {
            editor.selected = Some(index);
            cx.notify();
        }
    }
}

fn preview_is_tiny(annotation: &Annotation) -> bool {
    let Some(bounds) = annotation.bounds() else {
        return true;
    };
    bounds.w < 6.0 && bounds.h < 6.0
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

impl AppView {
    /// Keys are modal while the image editor is open.
    pub(crate) fn handle_image_editor_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let ctrl = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;

        if self.image_editor.as_ref().is_some_and(|editor| editor.text_entry.is_some()) {
            if key == "escape" {
                if let Some(editor) = self.image_editor.as_mut() {
                    editor.text_entry = None;
                }
                window.focus(&self.focus_handle);
                cx.notify();
                return;
            }
            if key == "enter" {
                self.image_editor_commit_text(cx);
                window.focus(&self.focus_handle);
                return;
            }
            if let Some(editor) = self.image_editor.as_mut() {
                if let Some(entry) = editor.text_entry.as_mut() {
                    entry.input.handle_key_down(event, cx);
                }
            }
            return;
        }

        if self.image_editor.as_ref().is_some_and(|editor| editor.confirm.is_some()) {
            if key == "escape" {
                if let Some(editor) = self.image_editor.as_mut() {
                    editor.confirm = None;
                }
                cx.notify();
            } else if key == "enter" {
                self.image_editor_confirm(cx);
            }
            return;
        }

        let active_field = self.image_editor.as_ref().and_then(|editor| editor.active_field);
        let active_ratio = self.image_editor.as_ref().and_then(|editor| editor.active_ratio);
        if let Some(index) = active_field {
            match key {
                "escape" => {
                    self.close_image_editor(false, window, cx);
                    return;
                }
                "enter" => {
                    self.close_image_editor(true, window, cx);
                    return;
                }
                "tab" => {
                    let next = (index + 1) % 4;
                    if let Some(editor) = self.image_editor.as_mut() {
                        editor.active_field = Some(next);
                        editor.active_ratio = None;
                        editor.crop_fields[next].select_all();
                        let handle = editor.crop_fields[next].focus_handle.clone();
                        if let Some(handle) = handle {
                            window.focus(&handle);
                        }
                    }
                    cx.notify();
                    return;
                }
                _ => {}
            }
            if let Some(editor) = self.image_editor.as_mut() {
                editor.crop_fields[index].handle_key_down(event, cx);
                let value = editor.crop_fields[index].text().trim().parse::<f32>().ok();
                let field = match index {
                    0 => CropField::X,
                    1 => CropField::Y,
                    2 => CropField::W,
                    _ => CropField::H,
                };
                if let Some(value) = value {
                    let _ = field;
                    let _ = value;
                }
            }
            if let Some(editor) = self.image_editor.as_ref() {
                let field = match index {
                    0 => CropField::X,
                    1 => CropField::Y,
                    2 => CropField::W,
                    _ => CropField::H,
                };
                if let Some(value) = editor.crop_fields[index].text().trim().parse::<f32>().ok() {
                    self.image_editor_set_field(field, value, cx);
                }
            }
            cx.notify();
            return;
        }
        if let Some(index) = active_ratio {
            if key == "escape" {
                if let Some(editor) = self.image_editor.as_mut() {
                    editor.active_ratio = None;
                    editor.ratio_fields[index].clear();
                }
                cx.notify();
                return;
            }
            if key == "tab" {
                let next = (index + 1) % 2;
                if let Some(editor) = self.image_editor.as_mut() {
                    editor.active_ratio = Some(next);
                    editor.active_field = None;
                    editor.ratio_fields[next].select_all();
                    let handle = editor.ratio_fields[next].focus_handle.clone();
                    if let Some(handle) = handle {
                        window.focus(&handle);
                    }
                }
                cx.notify();
                return;
            }
            if let Some(editor) = self.image_editor.as_mut() {
                editor.ratio_fields[index].handle_key_down(event, cx);
            }
            self.apply_custom_ratio(cx);
            return;
        }

        let mode = self.image_editor.as_ref().map(|editor| editor.mode);
        match key {
            "escape" => {
                self.close_image_editor(false, window, cx);
                return;
            }
            "enter" if mode == Some(ImageEditorMode::Crop) => {
                self.close_image_editor(true, window, cx);
                return;
            }
            _ => {}
        }
        if mode == Some(ImageEditorMode::Annotate) {
            if ctrl && (key.eq_ignore_ascii_case("z") || key == "я") {
                if event.keystroke.modifiers.shift {
                    self.image_editor_redo(cx);
                } else {
                    self.image_editor_undo(cx);
                }
                return;
            }
            if ctrl && (key.eq_ignore_ascii_case("y") || key == "н") {
                self.image_editor_redo(cx);
                return;
            }
            if key == "delete" || key == "backspace" {
                self.image_editor_delete_selected(cx);
                return;
            }
            if key == "enter" {
                self.close_image_editor(true, window, cx);
            }
        }
    }

    fn apply_custom_ratio(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        let w = editor.ratio_fields[0].text().trim().parse::<u32>().ok();
        let h = editor.ratio_fields[1].text().trim().parse::<u32>().ok();
        if let (Some(w), Some(h)) = (w, h) {
            if w > 0 && h > 0 {
                let aspect = CropAspect::Ratio(w, h);
                editor.aspect = aspect;
                let ratio = aspect.ratio(editor.natural).unwrap_or(1.0);
                let natural = editor.natural;
                let center_x = editor.frame.x + editor.frame.w / 2.0;
                let center_y = editor.frame.y + editor.frame.h / 2.0;
                let mut fw = editor.frame.w;
                let mut fh = fw / ratio;
                if fh > natural.1 as f32 {
                    fh = natural.1 as f32;
                    fw = fh * ratio;
                }
                if fw > natural.0 as f32 {
                    fw = natural.0 as f32;
                    fh = fw / ratio;
                }
                editor.frame = CropRect {
                    x: (center_x - fw / 2.0).clamp(0.0, (natural.0 as f32 - fw).max(0.0)),
                    y: (center_y - fh / 2.0).clamp(0.0, (natural.1 as f32 - fh).max(0.0)),
                    w: fw,
                    h: fh,
                };
                editor.edit.crop = Some(editor.frame);
                cx.notify();
            }
        }
    }

    pub fn image_editor_confirm(&mut self, cx: &mut Context<Self>) {
        let action = self.image_editor.as_mut().and_then(|editor| editor.confirm.take());
        match action {
            Some(ConfirmAction::ApplyCrop) => self.image_editor_apply_crop(cx),
            Some(ConfirmAction::Flatten) => self.image_editor_flatten(cx),
            Some(ConfirmAction::Blur(region)) => self.image_editor_blur(region, false, cx),
            Some(ConfirmAction::Pixelate(region)) => self.image_editor_blur(region, true, cx),
            None => {}
        }
    }

    pub fn open_image_editor_at_line(
        &mut self,
        mode: ImageEditorMode,
        line: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.state.active_document() else {
            return;
        };
        let text = doc.line_text(line);
        let wrap_cols = self.state.active_view().map(|view| view.window.wrap_cols).unwrap_or(80);
        let Some(image) = images::resolve_for_note(&doc.path, &text, wrap_cols, false) else {
            return;
        };
        self.open_image_editor(mode, line, &image, cx);
    }

    pub fn image_editor_matches(&self, line: usize, target: &str) -> bool {
        self.image_editor
            .as_ref()
            .is_some_and(|editor| editor.line == line && editor.target == target)
    }

    // ─────────────────────── Crop frame (in the row) ───────────────────────

    pub fn render_crop_frame_overlay(
        &self,
        image: &images::ResolvedImage,
        line: usize,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let Some(editor) = self.image_editor.as_ref() else {
            return div().into_any_element();
        };
        if editor.line != line || editor.target != image.reference.target {
            return div().into_any_element();
        }
        let scale = if image.visible.0 > 0 {
            image.width / image.visible.0 as f32
        } else {
            1.0
        };
        let width = image.width;
        let height = image.height;
        let frame = editor.frame;
        let fx = (frame.x * scale).clamp(0.0, width);
        let fy = (frame.y * scale).clamp(0.0, height);
        let fw = (frame.w * scale).clamp(0.0, width - fx);
        let fh = (frame.h * scale).clamp(0.0, height - fy);
        let shade = gpui::rgba(0x00000073);

        let mut layer = div()
            .id("crop-frame-overlay")
            .absolute()
            .inset_0()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(fx))
                    .h_full()
                    .bg(shade),
            )
            .child(
                div()
                    .absolute()
                    .left(px(fx + fw))
                    .top_0()
                    .w(px((width - fx - fw).max(0.0)))
                    .h_full()
                    .bg(shade),
            )
            .child(
                div()
                    .absolute()
                    .left(px(fx))
                    .top_0()
                    .w(px(fw))
                    .h(px(fy))
                    .bg(shade),
            )
            .child(
                div()
                    .absolute()
                    .left(px(fx))
                    .top(px(fy + fh))
                    .w(px(fw))
                    .h(px((height - fy - fh).max(0.0)))
                    .bg(shade),
            )
            .child(
                div()
                    .id("crop-frame")
                    .absolute()
                    .left(px(fx))
                    .top(px(fy))
                    .w(px(fw))
                    .h(px(fh))
                    .border_1()
                    .border_color(theme.accent)
                    .cursor(gpui::CursorStyle::OpenHand)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.image_editor_drag_start(
                                CropDragKind::Move,
                                event.position,
                                event.modifiers.shift,
                                cx,
                            );
                        }),
                    ),
            );

        let handles = [
            (CropHandle::Nw, fx, fy),
            (CropHandle::N, fx + fw / 2.0, fy),
            (CropHandle::Ne, fx + fw, fy),
            (CropHandle::E, fx + fw, fy + fh / 2.0),
            (CropHandle::Se, fx + fw, fy + fh),
            (CropHandle::S, fx + fw / 2.0, fy + fh),
            (CropHandle::Sw, fx, fy + fh),
            (CropHandle::W, fx, fy + fh / 2.0),
        ];
        for (handle, x, y) in handles {
            layer = layer.child(
                div()
                    .id(gpui::ElementId::Name(
                        SharedString::from(format!("crop-handle-{:?}", handle)),
                    ))
                    .absolute()
                    .left(px(x - 5.0))
                    .top(px(y - 5.0))
                    .w(px(10.0))
                    .h(px(10.0))
                    .bg(gpui::rgb(0xffffff))
                    .border_1()
                    .border_color(theme.accent)
                    .rounded_sm()
                    .cursor(gpui::CursorStyle::Crosshair)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.image_editor_drag_start(
                                CropDragKind::Handle(handle),
                                event.position,
                                event.modifiers.shift,
                                cx,
                            );
                        }),
                    ),
            );
        }
        layer.into_any_element()
    }

    // ─────────────────────── Overlay panels ───────────────────────

    pub fn render_image_editor_overlay(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let Some(editor) = self.image_editor.as_ref() else {
            return div().into_any_element();
        };
        let (rx, ry, rw, rh) = editor.rect.get();
        let window_w = f32::from(window.bounds().size.width);
        let window_h = f32::from(window.bounds().size.height);
        let panel_x = if rx + rw + PANEL_W + 16.0 <= window_w {
            rx + rw + 12.0
        } else if rx - PANEL_W - 12.0 >= 8.0 {
            rx - PANEL_W - 12.0
        } else {
            (window_w - PANEL_W - 8.0).max(8.0)
        };
        let panel_y = ry.clamp(8.0, (window_h - 420.0).max(8.0));

        let mut root = div()
            .id("image-editor-overlay")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_key(event, window, cx);
            }));

        if editor.drag.is_some() {
            root = root.child(
                div()
                    .id("crop-drag-capture")
                    .absolute()
                    .inset_0()
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        this.image_editor_drag_move(event.position, event.modifiers.shift, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.image_editor_drag_end(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.image_editor_drag_end(cx)),
                    ),
            );
        }
        if editor.annotation_drag.is_some() {
            root = root.child(
                div()
                    .id("annotation-drag-capture")
                    .absolute()
                    .inset_0()
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        this.image_editor_pointer_move(event.position, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.image_editor_pointer_up(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.image_editor_pointer_up(cx)),
                    ),
            );
        }

        if let Some(entry) = editor.text_entry.as_ref() {
            let entry_focus = entry.focus.clone();
            let scale = editor.scale.get().max(0.0001);
            let left = (rx + entry.point.x * scale).clamp(8.0, (window_w - 240.0).max(8.0));
            let top = (ry + entry.point.y * scale).clamp(8.0, (window_h - 46.0).max(8.0));
            root = root.child(
                div()
                    .id("annotation-text-entry")
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(232.0))
                    .h(px(31.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded_md()
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.accent)
                    .track_focus(&entry.focus)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, _, window, cx| {
                            cx.stop_propagation();
                            window.focus(&entry_focus);
                        }),
                    )
                    .child(entry.input.render(
                        &crate::i18n::t("editor.image.text_placeholder", &self.config.ui.language),
                        theme,
                        window,
                        window.scale_factor(),
                        13.0,
                        "Segoe UI",
                    )),
            );
        }

        match editor.mode {
            ImageEditorMode::Crop => {
                root = root.child(
                    div()
                        .absolute()
                        .left(px(panel_x))
                        .top(px(panel_y))
                        .child(self.render_crop_panel(theme, window, cx)),
                );
            }
            ImageEditorMode::Annotate => {
                let toolbar_y = if ry >= 58.0 {
                    ry - 50.0
                } else {
                    (ry + rh + 8.0).min((window_h - 50.0).max(8.0))
                };
                root = root.child(
                    div()
                        .absolute()
                        .left(px(rx.max(8.0)))
                        .top(px(toolbar_y.max(8.0)))
                        .child(self.render_annotation_toolbar(theme, cx)),
                );
                root = root.child(
                    div()
                        .absolute()
                        .left(px(panel_x))
                        .top(px(panel_y))
                        .child(self.render_annotation_panel(theme, cx)),
                );
            }
        }

        if let Some(confirm) = editor.confirm {
            root = root.child(self.render_image_confirm(confirm, theme, cx));
        }
        root.into_any_element()
    }

    fn render_crop_panel(&self, theme: &Theme, window: &Window, cx: &Context<Self>) -> Div {
        let Some(editor) = self.image_editor.as_ref() else {
            return div();
        };
        let lang = self.config.ui.language.as_str();
        let t = |key: &str| crate::i18n::t(key, lang);
        let pixel_scale = window.scale_factor();

        let mut aspects = div().flex().flex_row().flex_wrap().gap(px(4.0));
        let presets = [
            (CropAspect::Free, t("editor.image.crop_free")),
            (CropAspect::Original, t("editor.image.crop_original")),
            (CropAspect::Ratio(1, 1), "1:1".to_string()),
            (CropAspect::Ratio(4, 3), "4:3".to_string()),
            (CropAspect::Ratio(3, 2), "3:2".to_string()),
            (CropAspect::Ratio(16, 9), "16:9".to_string()),
            (CropAspect::Ratio(10, 15), "10:15".to_string()),
            (CropAspect::Ratio(2, 3), "2:3".to_string()),
            (CropAspect::Ratio(3, 4), "3:4".to_string()),
        ];
        for (aspect, label) in presets {
            let selected = editor.aspect == aspect;
            aspects = aspects.child(
                div()
                    .id(gpui::ElementId::Name(
                        SharedString::from(format!("crop-aspect-{}", label)),
                    ))
                    .h(px(24.0))
                    .px(px(7.0))
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .border_1()
                    .border_color(if selected { theme.accent } else { theme.border })
                    .bg(if selected { theme.bg_selected } else { theme.bg_sidebar })
                    .text_xs()
                    .text_color(if selected { theme.accent } else { theme.text_secondary })
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.image_editor_set_aspect(aspect, cx);
                        }),
                    )
                    .child(label),
            );
        }

        let ratio_field = |index: usize| {
            let input = &editor.ratio_fields[index];
            div()
                .h(px(26.0))
                .w(px(52.0))
                .px(px(6.0))
                .flex()
                .items_center()
                .rounded_md()
                .bg(theme.bg_sidebar)
                .border_1()
                .border_color(if editor.active_ratio == Some(index) {
                    theme.accent
                } else {
                    theme.border
                })
                .cursor(gpui::CursorStyle::IBeam)
                .when_some(input.focus_handle.as_ref(), |d, handle| d.track_focus(handle))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        if let Some(editor) = this.image_editor.as_mut() {
                            editor.active_ratio = Some(index);
                            editor.active_field = None;
                            editor.ratio_fields[index].select_all();
                            let handle = editor.ratio_fields[index].focus_handle.clone();
                            if let Some(handle) = handle {
                                window.focus(&handle);
                            }
                        }
                        cx.notify();
                    }),
                )
                .child(input.render("0", theme, window, pixel_scale, 12.0, "Segoe UI"))
        };

        let field = |index: usize, label: String| {
            let input = &editor.crop_fields[index];
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                // Two fields per row with slack; the exact half width wrapped to one column.
                .w(px((PANEL_W - 24.0 - 8.0) / 2.0 - 6.0))
                .child(div().text_xs().text_color(theme.text_muted).child(label))
                .child(
                    div()
                        .h(px(26.0))
                        .px(px(6.0))
                        .flex()
                        .items_center()
                        .rounded_md()
                        .bg(theme.bg_sidebar)
                        .border_1()
                        .border_color(if editor.active_field == Some(index) {
                            theme.accent
                        } else {
                            theme.border
                        })
                        .cursor(gpui::CursorStyle::IBeam)
                        .when_some(input.focus_handle.as_ref(), |d, handle| d.track_focus(handle))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                if let Some(editor) = this.image_editor.as_mut() {
                                    editor.active_field = Some(index);
                                    editor.active_ratio = None;
                                    editor.crop_fields[index].select_all();
                                    let handle = editor.crop_fields[index].focus_handle.clone();
                                    if let Some(handle) = handle {
                                        window.focus(&handle);
                                    }
                                }
                                cx.notify();
                            }),
                        )
                        .child(input.render("0", theme, window, pixel_scale, 13.0, "Segoe UI")),
                )
        };

        let caption = div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(t("editor.image.crop_title")),
            )
            .child(self.image_close_button(
                "crop-close-x",
                theme,
                cx.listener(|this, _, window, cx| this.close_image_editor(false, window, cx)),
            ));

        div()
            .w(px(PANEL_W))
            .flex()
            .flex_col()
            .gap(px(9.0))
            .p(px(12.0))
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_key(event, window, cx);
            }))
            .child(caption)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(div().text_xs().text_color(theme.text_muted).child(t("editor.image.crop_aspect")))
                    .child(
                        div()
                            .id("crop-aspect-swap")
                            .h(px(22.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded_sm()
                            .bg(theme.bg_sidebar)
                            .border_1()
                            .border_color(theme.border)
                            .text_xs()
                            .text_color(theme.text_secondary)
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.image_editor_swap_aspect(cx);
                                }),
                            )
                            .child(t("editor.image.crop_swap")),
                    ),
            )
            .child(aspects)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().text_xs().text_color(theme.text_muted).child(t("editor.image.crop_custom")))
                    .child(ratio_field(0))
                    .child(div().text_xs().text_color(theme.text_muted).child(":"))
                    .child(ratio_field(1)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(field(0, t("editor.image.crop_x")))
                    .child(field(1, t("editor.image.crop_y")))
                    .child(field(2, t("editor.image.crop_w")))
                    .child(field(3, t("editor.image.crop_h"))),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(t("editor.image.crop_warning")),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(6.0))
                    .child(self.editor_text_button(
                        "crop-reset",
                        t("editor.image.crop_reset"),
                        theme,
                        false,
                        cx.listener(|this, _, _, cx| this.image_editor_reset_crop(cx)),
                    ))
                    .child(self.editor_text_button(
                        "crop-apply",
                        t("editor.image.crop_apply"),
                        theme,
                        true,
                        cx.listener(|this, _, _, cx| {
                            if let Some(editor) = this.image_editor.as_mut() {
                                editor.confirm = Some(ConfirmAction::ApplyCrop);
                            }
                            cx.notify();
                        }),
                    ))
                    .child(self.editor_text_button(
                        "crop-cancel",
                        t("editor.image.crop_cancel"),
                        theme,
                        false,
                        cx.listener(|this, _, window, cx| this.close_image_editor(false, window, cx)),
                    ))
                    .child(self.editor_text_button(
                        "crop-done",
                        t("editor.image.crop_done"),
                        theme,
                        true,
                        cx.listener(|this, _, window, cx| this.close_image_editor(true, window, cx)),
                    )),
            )
    }

    fn render_annotation_toolbar(&self, theme: &Theme, cx: &Context<Self>) -> Div {
        let Some(editor) = self.image_editor.as_ref() else {
            return div();
        };
        let lang = self.config.ui.language.as_str();
        let tools = [
            (AnnotationTool::Select, icons::ICON_MOVE_ARROWS, "editor.image.tool_select"),
            (AnnotationTool::Arrow, icons::ICON_ARROW_RIGHT, "editor.image.tool_arrow"),
            (AnnotationTool::Line, icons::ICON_MINIMIZE, "editor.image.tool_line"),
            (AnnotationTool::Rect, icons::ICON_MAXIMIZE, "editor.image.tool_rect"),
            (AnnotationTool::Ellipse, icons::ICON_CIRCLE_PLUS, "editor.image.tool_ellipse"),
            (AnnotationTool::Highlight, icons::ICON_HIGHLIGHTER, "editor.image.tool_highlight"),
            (AnnotationTool::Pencil, icons::ICON_PENCIL, "editor.image.tool_pencil"),
            (AnnotationTool::Text, icons::ICON_TYPE, "editor.image.tool_text"),
            (AnnotationTool::Label, icons::ICON_HASH, "editor.image.tool_label"),
            (AnnotationTool::Blur, icons::ICON_ERASER, "editor.image.tool_blur"),
            (AnnotationTool::Pixelate, icons::ICON_CANVAS, "editor.image.tool_pixelate"),
        ];
        let mut bar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.0))
            .px(px(6.0))
            .h(px(38.0))
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_key(event, window, cx);
            }));
        for (tool, icon, _key) in tools {
            let selected = editor.tool == tool;
            bar = bar.child(
                div()
                    .id(gpui::ElementId::Name(
                        SharedString::from(format!("ann-tool-{:?}", tool)),
                    ))
                    .w(px(28.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .when(selected, |d| d.bg(theme.bg_selected))
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(editor) = this.image_editor.as_mut() {
                                editor.tool = tool;
                                editor.selected = None;
                            }
                            cx.notify();
                        }),
                    )
                    .child(icons::icon(
                        icon,
                        14.0,
                        if selected { theme.accent } else { theme.text_secondary },
                    )),
            );
        }
        bar = bar.child(div().w(px(1.0)).h(px(20.0)).mx(px(3.0)).bg(theme.border));
        for color in PALETTE {
            let selected = editor.color.eq_ignore_ascii_case(color);
            let rgba = images::parse_hex_color(color);
            bar = bar.child(
                div()
                    .id(gpui::ElementId::Name(
                        SharedString::from(format!("ann-color-{}", color)),
                    ))
                    .w(px(16.0))
                    .h(px(16.0))
                    .rounded_full()
                    .bg(gpui::rgba(
                        (rgba[0] as u32) << 24
                            | (rgba[1] as u32) << 16
                            | (rgba[2] as u32) << 8
                            | rgba[3] as u32,
                    ))
                    .when(selected, |d| d.border_2().border_color(theme.accent))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(editor) = this.image_editor.as_mut() {
                                editor.color = color.to_string();
                            }
                            cx.notify();
                        }),
                    ),
            );
        }
        bar = bar.child(div().w(px(1.0)).h(px(20.0)).mx(px(3.0)).bg(theme.border));
        bar = bar.child(
            div()
                .id("ann-width-down")
                .w(px(20.0))
                .h(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .cursor_pointer()
                .hover(|h| h.bg(theme.bg_hover))
                .child("-")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(editor) = this.image_editor.as_mut() {
                            editor.width = (editor.width - 1.0).clamp(1.0, 40.0);
                        }
                        cx.notify();
                    }),
                ),
        );
        bar = bar.child(
            div()
                .w(px(30.0))
                .text_xs()
                .text_color(theme.text_secondary)
                .text_center()
                .child(format!("{:.0}", editor.width)),
        );
        bar = bar.child(
            div()
                .id("ann-width-up")
                .w(px(20.0))
                .h(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .cursor_pointer()
                .hover(|h| h.bg(theme.bg_hover))
                .child("+")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(editor) = this.image_editor.as_mut() {
                            editor.width = (editor.width + 1.0).clamp(1.0, 40.0);
                        }
                        cx.notify();
                    }),
                ),
        );
        bar = bar.child(
            div()
                .id("ann-fill")
                .h(px(24.0))
                .px(px(7.0))
                .flex()
                .items_center()
                .rounded_sm()
                .bg(if editor.filled { theme.bg_selected } else { theme.bg_sidebar })
                .border_1()
                .border_color(if editor.filled { theme.accent } else { theme.border })
                .text_xs()
                .text_color(theme.text_secondary)
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(editor) = this.image_editor.as_mut() {
                            editor.filled = !editor.filled;
                        }
                        cx.notify();
                    }),
                )
                .child(crate::i18n::t("editor.image.fill", lang)),
        );
        bar = bar.child(div().w(px(1.0)).h(px(20.0)).mx(px(3.0)).bg(theme.border));
        bar = bar.child(self.editor_icon_button(
            "ann-undo",
            icons::ICON_UNDO,
            !editor.undo.is_empty(),
            theme,
            cx.listener(|this, _, _, cx| this.image_editor_undo(cx)),
        ));
        bar = bar.child(self.editor_icon_button(
            "ann-redo",
            icons::ICON_REDO,
            !editor.redo.is_empty(),
            theme,
            cx.listener(|this, _, _, cx| this.image_editor_redo(cx)),
        ));
        bar = bar.child(
            div()
                .id("ann-done")
                .h(px(26.0))
                .px(px(9.0))
                .flex()
                .items_center()
                .rounded_md()
                .bg(theme.accent)
                .text_xs()
                .text_color(theme.text_on_accent)
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_image_editor(true, window, cx);
                    }),
                )
                .child(crate::i18n::t("editor.image.crop_done", lang)),
        );
        bar = bar.child(
            div()
                .id("ann-cancel")
                .h(px(26.0))
                .px(px(9.0))
                .flex()
                .items_center()
                .rounded_md()
                .bg(theme.bg_sidebar)
                .border_1()
                .border_color(theme.border)
                .text_xs()
                .text_color(theme.text_secondary)
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_image_editor(false, window, cx);
                    }),
                )
                .child(crate::i18n::t("editor.image.crop_cancel", lang)),
        );
        bar
    }

    fn render_annotation_panel(&self, theme: &Theme, cx: &Context<Self>) -> Div {
        let Some(editor) = self.image_editor.as_ref() else {
            return div();
        };
        let lang = self.config.ui.language.as_str();
        let mut layers = div().flex().flex_col().gap(px(2.0)).max_h(px(250.0)).overflow_hidden();
        if editor.edit.annotations.is_empty() {
            layers = layers.child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(crate::i18n::t("editor.image.layers", lang)),
            );
        }
        for (index, annotation) in editor.edit.annotations.iter().enumerate().rev() {
            let selected = editor.selected == Some(index);
            let label = annotation_label(annotation.kind, lang);
            layers = layers.child(
                div()
                    .id(gpui::ElementId::Name(
                        SharedString::from(format!("ann-layer-{}", index)),
                    ))
                    .h(px(24.0))
                    .px(px(6.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.0))
                    .rounded_sm()
                    .when(selected, |d| d.bg(theme.bg_selected))
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.image_editor_select_layer(index, cx);
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(if selected { theme.accent } else { theme.text_secondary })
                            .child(label),
                    )
                    .child(
                        div()
                            .id(gpui::ElementId::Name(
                                SharedString::from(format!("ann-layer-up-{}", index)),
                            ))
                            .w(px(18.0))
                            .text_xs()
                            .text_color(theme.text_muted)
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.image_editor_move_layer(index, 1, cx);
                                }),
                            )
                            .child("↑"),
                    )
                    .child(
                        div()
                            .id(gpui::ElementId::Name(
                                SharedString::from(format!("ann-layer-down-{}", index)),
                            ))
                            .w(px(18.0))
                            .text_xs()
                            .text_color(theme.text_muted)
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.image_editor_move_layer(index, -1, cx);
                                }),
                            )
                            .child("↓"),
                    )
                    .child(
                        div()
                            .id(gpui::ElementId::Name(
                                SharedString::from(format!("ann-layer-del-{}", index)),
                            ))
                            .w(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(editor) = this.image_editor.as_mut() {
                                        editor.selected = Some(index);
                                    }
                                    this.image_editor_delete_selected(cx);
                                }),
                            )
                            .child(icons::icon(icons::ICON_TRASH_2, 11.0, theme.text_muted)),
                    ),
            );
        }

        div()
            .w(px(PANEL_W))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_key(event, window, cx);
            }))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(crate::i18n::t("editor.image.annotate_title", lang)),
                    )
                    .child(self.image_close_button(
                        "annotate-close-x",
                        theme,
                        cx.listener(|this, _, window, cx| {
                            this.close_image_editor(false, window, cx)
                        }),
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(crate::i18n::t("editor.image.layers", lang)),
            )
            .child(layers)
            .child(self.editor_text_button(
                "ann-flatten",
                crate::i18n::t("editor.image.flatten", lang),
                theme,
                false,
                cx.listener(|this, _, _, cx| {
                    if let Some(editor) = this.image_editor.as_mut() {
                        editor.confirm = Some(ConfirmAction::Flatten);
                    }
                    cx.notify();
                }),
            ))
    }

    fn render_image_confirm(
        &self,
        action: ConfirmAction,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let lang = self.config.ui.language.as_str();
        let message = match action {
            ConfirmAction::ApplyCrop => crate::i18n::t("editor.image.confirm_crop", lang),
            ConfirmAction::Flatten => crate::i18n::t("editor.image.confirm_flatten", lang),
            ConfirmAction::Blur(_) => crate::i18n::t("editor.image.confirm_blur", lang),
            ConfirmAction::Pixelate(_) => crate::i18n::t("editor.image.confirm_pixelate", lang),
        };
        div()
            .id("image-confirm-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000059))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
            }))
            .child(
                div()
                    .w(px(360.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .p(px(16.0))
                    .rounded_lg()
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_primary)
                            .child(message),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap(px(6.0))
                            .child(self.editor_text_button(
                                "confirm-cancel",
                                crate::i18n::t("editor.image.crop_cancel", lang),
                                theme,
                                false,
                                cx.listener(|this, _, _, cx| {
                                    if let Some(editor) = this.image_editor.as_mut() {
                                        editor.confirm = None;
                                    }
                                    cx.notify();
                                }),
                            ))
                            .child(self.editor_text_button(
                                "confirm-apply",
                                crate::i18n::t("editor.image.apply", lang),
                                theme,
                                true,
                                cx.listener(|this, _, _, cx| this.image_editor_confirm(cx)),
                            )),
                    ),
            )
    }

    fn editor_text_button(
        &self,
        id: &'static str,
        label: String,
        theme: &Theme,
        primary: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> gpui::Stateful<Div> {
        div()
            .id(gpui::ElementId::Name(SharedString::from(id)))
            .h(px(26.0))
            .px(px(9.0))
            .flex()
            .items_center()
            .rounded_md()
            .bg(if primary { theme.accent } else { theme.bg_sidebar })
            .border_1()
            .border_color(if primary { theme.accent } else { theme.border })
            .text_xs()
            .text_color(if primary { theme.text_on_accent } else { theme.text_secondary })
            .cursor_pointer()
            .hover(|h| h.bg(if primary { theme.accent } else { theme.bg_hover }))
            .on_mouse_down(MouseButton::Left, listener)
            .child(label)
    }

    fn editor_icon_button(
        &self,
        id: &'static str,
        icon: &'static str,
        enabled: bool,
        theme: &Theme,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> gpui::Stateful<Div> {
        div()
            .id(gpui::ElementId::Name(SharedString::from(id)))
            .w(px(26.0))
            .h(px(26.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .when(enabled, |d| {
                d.cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .when(!enabled, |d| d.opacity(0.35).cursor_not_allowed())
            .child(icons::icon(
                icon,
                14.0,
                if enabled { theme.text_secondary } else { theme.text_muted },
            ))
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

impl AppView {
    pub fn open_caption_editor_at_line(
        &mut self,
        line: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.state.active_document() else {
            return;
        };
        let text = doc.line_text(line);
        let wrap_cols = self.state.active_view().map(|view| view.window.wrap_cols).unwrap_or(80);
        let Some(image) = images::resolve_for_note(&doc.path, &text, wrap_cols, false) else {
            return;
        };
        self.open_caption_editor(line, &image, window, cx);
    }

    pub fn open_caption_editor(
        &mut self,
        line: usize,
        image: &images::ResolvedImage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.image_editor.is_some() {
            return;
        }
        let caption = image.edit.caption.clone().unwrap_or_else(|| images::Caption {
            text: String::new(),
            position: self.config.images.caption_position.clone(),
            numbered: self.config.images.caption_numbered,
        });
        // The editor container listens for keys on AppView's stored handle,
        // so the input must use that exact handle, not a fresh one.
        let focus = self.focus_handle.clone();
        let mut input = TextInput::new(focus.clone());
        input.set_text(caption.text.clone());
        input.select_all();
        window.focus(&focus);
        self.caption_editor = Some(CaptionEditorState {
            line,
            path: image.path.clone(),
            edit: image.edit.clone(),
            input,
            focus,
            position: if caption.position.is_empty() {
                self.config.images.caption_position.clone()
            } else {
                caption.position
            },
            numbered: caption.numbered,
            position_menu_open: false,
        });
        self.pending_caption_reveal = Some(line);
        self.select_image_line(line);
        cx.notify();
    }

    pub fn save_caption_editor(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.caption_editor.take() else {
            return;
        };
        let mut edit = state.edit;
        let text = state.input.text().trim().to_string();
        edit.caption = if text.is_empty() {
            None
        } else {
            Some(images::Caption {
                text,
                position: state.position,
                numbered: state.numbered,
            })
        };
        let _ = images::save_edit(&state.path, &edit);
        images::invalidate_resolved_images();
        if let Some(view) = self.state.active_view_mut() {
            view.visual_cache_dirty = true;
        }
        self.clear_image_selection();
        cx.notify();
    }

    pub fn cancel_caption_editor(&mut self, cx: &mut Context<Self>) {
        if self.caption_editor.take().is_some() {
            self.clear_image_selection();
            cx.notify();
        }
    }

    pub(crate) fn handle_caption_editor_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            if self
                .caption_editor
                .as_ref()
                .is_some_and(|state| state.position_menu_open)
            {
                if let Some(state) = self.caption_editor.as_mut() {
                    state.position_menu_open = false;
                }
                cx.notify();
                return;
            }
            self.cancel_caption_editor(cx);
            window.focus(&self.focus_handle);
            return;
        }
        if key == "enter" {
            self.save_caption_editor(cx);
            window.focus(&self.focus_handle);
            return;
        }
        if let Some(state) = self.caption_editor.as_mut() {
            state.input.handle_key_down(event, cx);
        }
    }

    /// Inline caption editor: the field sits on the caption line itself.
    /// Position and numbering controls share that line; the text is committed
    /// on Enter or a click outside, Esc restores the saved caption.
    pub fn render_caption_inline_editor(
        &self,
        image: &images::ResolvedImage,
        line: usize,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let Some(state) = self
            .caption_editor
            .as_ref()
            .filter(|state| state.line == line && state.path == image.path)
        else {
            return div().into_any_element();
        };
        let lang = self.config.ui.language.as_str();
        let above = state.position == "above";
        let menu_open = state.position_menu_open;
        let position_label = crate::i18n::t(
            if above {
                "editor.image.caption_above_image"
            } else {
                "editor.image.caption_below_image"
            },
            lang,
        );

        let option = |id: &'static str, label: String, selected: bool, is_above: bool| {
            div()
                .id(gpui::ElementId::Name(SharedString::from(id)))
                .h(px(26.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded_sm()
                .when(selected, |d| d.bg(theme.bg_selected))
                .text_size(px(12.0))
                .text_color(if selected { theme.accent } else { theme.text_secondary })
                .cursor_pointer()
                .hover(|h| h.bg(theme.bg_hover))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(state) = this.caption_editor.as_mut() {
                            state.position = if is_above { "above" } else { "below" }.to_string();
                            state.position_menu_open = false;
                        }
                        cx.notify();
                    }),
                )
                .child(label)
        };

        let position_select = div()
            .relative()
            .w(px(132.0))
            .h(px(34.0))
            .flex_shrink_0()
            .child(
                div()
                    .id("caption-position-select")
                    .size_full()
                    .px(px(8.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(4.0))
                    .rounded_md()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .text_size(px(12.0))
                    .text_color(theme.text_primary)
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(state) = this.caption_editor.as_mut() {
                                state.position_menu_open = !state.position_menu_open;
                            }
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .h(px(16.0))
                            .line_height(px(15.0))
                            .font_family("Segoe UI")
                            .child(position_label.clone()),
                    )
                    .child(icons::icon(icons::ICON_CHEVRON_DOWN, 12.0, theme.text_muted)),
            )
            .when(menu_open, |d| {
                d.child(
                    div()
                        .id("caption-position-menu")
                        .absolute()
                        .top(px(37.0))
                        .left_0()
                        .w(px(132.0))
                        .p(px(3.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .rounded_md()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border)
                        .child(option(
                            "caption-pos-above",
                            crate::i18n::t("editor.image.caption_above_image", lang),
                            above,
                            true,
                        ))
                        .child(option(
                            "caption-pos-below",
                            crate::i18n::t("editor.image.caption_below_image", lang),
                            !above,
                            false,
                        )),
                )
            });

        let numbered = state.numbered;
        let numbered_toggle = div()
            .id("caption-numbered-toggle")
            .h(px(34.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .flex_shrink_0()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    if let Some(state) = this.caption_editor.as_mut() {
                        state.numbered = !state.numbered;
                    }
                    cx.notify();
                }),
            )
            .child(
                div()
                    .w(px(28.0))
                    .h(px(16.0))
                    .rounded_full()
                    .bg(if numbered { theme.accent } else { theme.bg_selected })
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .top(px(2.0))
                            .left(px(if numbered { 14.0 } else { 2.0 }))
                            .w(px(12.0))
                            .h(px(12.0))
                            .rounded_full()
                            .bg(if numbered { theme.text_on_accent } else { theme.text_muted }),
                    ),
            )
            .child(
                div()
                    .h(px(16.0))
                    .line_height(px(15.0))
                    .text_size(px(12.0))
                    .font_family("Segoe UI")
                    .text_color(theme.text_primary)
                    .child(crate::i18n::t("editor.image.numbered", lang)),
            );

        div()
            .id("caption-inline-editor")
            .w(px(image.width))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            // A little air under the line, plus room for the dropdown list.
            .pb(px(if menu_open { 62.0 } else { 6.0 }))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
            }))
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.save_caption_editor(cx);
                window.focus(&this.focus_handle);
            }))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_key(event, window, cx);
            }))
            .child(
                div()
                    .id("caption-inline-field")
                    .h(px(34.0))
                    .flex_1()
                    .min_w_0()
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded_md()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.accent)
                    .track_focus(&state.focus)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            if let Some(state) = this.caption_editor.as_ref() {
                                window.focus(&state.focus);
                            }
                        }),
                    )
                    .child(state.input.render(
                        &crate::i18n::t("editor.image.caption_title", lang),
                        theme,
                        window,
                        window.scale_factor(),
                        12.0,
                        "Segoe UI",
                    )),
            )
            .child(
                div()
                    .h(px(16.0))
                    .line_height(px(15.0))
                    .text_size(px(12.0))
                    .font_family("Segoe UI")
                    .text_color(theme.text_secondary)
                    .flex_shrink_0()
                    .child(crate::i18n::t("editor.image.caption_position", lang)),
            )
            .child(position_select)
            .child(numbered_toggle)
            .into_any_element()
    }
}
