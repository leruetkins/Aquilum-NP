//! Inline image block, its hover toolbar, and the size/radius popovers.
//!
//! The block itself is painted by `render_central_editor`; everything that
//! needs its own state (hover line, selected line, open popover) lives here so
//! the editor loop stays readable.

use std::path::{Path, PathBuf};

use gpui::{
    div, prelude::*, px, AnyElement, Context, Div, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, ScrollWheelEvent, SharedString, Stateful, StyledImage, Window,
};

use crate::icons;
use crate::images::{self, ImageImportSource, ImageReference, ImportedImage, ResolvedImage};
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::notification::NotificationType;
use crate::views::text_input::TextInput;

mod actions;
mod popover;
mod render;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePopoverKind {
    Size,
    Radius,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeField {
    Width,
    Height,
}

/// Floating editor anchored at the toolbar button that opened it.
pub struct ImagePopoverState {
    pub kind: ImagePopoverKind,
    pub line: usize,
    pub path: PathBuf,
    pub reference: ImageReference,
    pub natural: (u32, u32),
    /// Size of the shown region (after crop). Aspect ratio for the lock must
    /// use this, not the full original.
    pub visible: (u32, u32),
    pub wrap_cols: usize,
    pub x: f32,
    pub y: f32,
    /// Focus handle of the panel itself, so the radius editor (which has no
    /// text field) still receives keys through the panel's `on_key_down`.
    pub focus: gpui::FocusHandle,
    pub width: TextInput,
    pub height: TextInput,
    pub active: SizeField,
    pub lock: bool,
}

impl ImagePopoverState {
    pub fn active_input(&self) -> &TextInput {
        match self.active {
            SizeField::Width => &self.width,
            SizeField::Height => &self.height,
        }
    }

    pub fn active_input_mut(&mut self) -> &mut TextInput {
        match self.active {
            SizeField::Width => &mut self.width,
            SizeField::Height => &mut self.height,
        }
    }
}

/// Scale that makes the whole picture fit the viewer, limited by its larger
/// side. Small pictures stay at 100% instead of being blown up.
fn fit_scale(natural: (u32, u32), available: (f32, f32)) -> f32 {
    let nw = natural.0.max(1) as f32;
    let nh = natural.1.max(1) as f32;
    (available.0 / nw)
        .min(available.1 / nh)
        .min(1.0)
        .clamp(0.05, 8.0)
}

/// Resolves the reference size written from the Size popover. With the lock
/// on, the field the user last edited drives the other dimension; editing
/// height must therefore change width instead of being ignored.
fn locked_display_size(
    active: SizeField,
    raw_width: Option<u32>,
    raw_height: Option<u32>,
    visible: (u32, u32),
) -> (Option<u32>, Option<u32>) {
    if active == SizeField::Height {
        let ratio = if visible.1 > 0 {
            visible.0 as f32 / visible.1 as f32
        } else {
            1.0
        };
        let width = raw_height
            .map(|height| ((height as f32) * ratio).round().max(1.0) as u32)
            .or(raw_width);
        (width, None)
    } else {
        (raw_width, None)
    }
}

/// Full-window preview of the original image file. `scale` is the on-screen
/// size of one source pixel: 1.0 means the picture is shown at its natural
/// resolution, and the wheel changes it. `pan` moves a zoomed picture around.
pub struct ImageViewerState {
    pub path: PathBuf,
    pub title: String,
    pub natural: (u32, u32),
    pub scale: f32,
    /// Default fitted scale, restored by double-click and the `0` key.
    pub fit: f32,
    pub pan: (f32, f32),
    pub drag: Option<(f32, f32, f32, f32)>,
}

pub struct SizeDrag {
    pub line: usize,
    pub max_width: f32,
    pub start: (f32, f32),
    pub start_size: (f32, f32),
    pub current: (f32, f32),
    pub sign: (f32, f32),
    pub lock: bool,
}

const POPOVER_W: f32 = 268.0;
const RADIUS_TRACK_W: f32 = 196.0;
/// The inline caption editor keeps this much room even under a tiny picture.
const CAPTION_EDITOR_MIN_W: f32 = 360.0;



/// Paints the shared vector geometry with GPUI. `scale` converts original
/// image pixels to the displayed picture; `offset` is the crop origin.
fn paint_annotation_paths(
    window: &mut Window,
    annotations: &[crate::images::Annotation],
    scale: f32,
    offset: (f32, f32),
    origin: gpui::Point<gpui::Pixels>,
) {
    for annotation in annotations {
        let mut color = crate::images::parse_hex_color(&annotation.color);
        let mut stroke_width = annotation.width.max(0.5) * scale;
        if matches!(annotation.kind, crate::images::AnnotationKind::Highlight) {
            color[3] = color[3].min(140);
            stroke_width *= 3.0;
        }
        for shape in crate::images::annotation_geometry(annotation) {
            let mut builder = if shape.filled {
                gpui::PathBuilder::fill()
            } else {
                gpui::PathBuilder::stroke(px(stroke_width))
            };
            for (index, point) in shape.points.iter().enumerate() {
                let x = f32::from(origin.x) + (point.x - offset.0) * scale;
                let y = f32::from(origin.y) + (point.y - offset.1) * scale;
                if index == 0 {
                    builder.move_to(gpui::point(px(x), px(y)));
                } else {
                    builder.line_to(gpui::point(px(x), px(y)));
                }
            }
            if shape.closed {
                builder.close();
            }
            if let Ok(path) = builder.build() {
                window.paint_path(
                    path,
                    gpui::rgba((color[0] as u32) << 24 | (color[1] as u32) << 16 | (color[2] as u32) << 8 | color[3] as u32),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_starts_fitted_by_the_larger_side() {
        assert_eq!(fit_scale((4000, 2000), (2000.0, 1000.0)), 0.5);
        assert_eq!(fit_scale((2000, 4000), (2000.0, 1000.0)), 0.25);
        assert_eq!(fit_scale((800, 400), (2000.0, 1000.0)), 1.0);
    }

    #[test]
    fn locked_size_uses_the_edited_field() {
        let visible = (800, 400);
        assert_eq!(
            locked_display_size(SizeField::Width, Some(300), Some(200), visible),
            (Some(300), None)
        );
        assert_eq!(
            locked_display_size(SizeField::Height, Some(300), Some(100), visible),
            (Some(200), None)
        );
        assert_eq!(
            locked_display_size(SizeField::Height, Some(300), None, visible),
            (Some(300), None)
        );
    }
}
