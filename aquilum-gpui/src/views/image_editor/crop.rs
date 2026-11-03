use super::*;

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

    pub(super) fn sync_crop_frame(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn push_undo(&mut self) {
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
        shift: bool,
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
                        let mut next = point;
                        // Shift keeps the original width/height ratio while a
                        // rectangle or ellipse corner is dragged.
                        if shift
                            && matches!(annotation.kind, AnnotationKind::Rect | AnnotationKind::Ellipse)
                            && drag.origin.len() >= 2
                        {
                            let anchor = drag.origin[1 - handle.min(1)];
                            let origin_w = (drag.origin[1].x - drag.origin[0].x).abs();
                            let origin_h = (drag.origin[1].y - drag.origin[0].y).abs();
                            if origin_w > 0.5 && origin_h > 0.5 {
                                let ratio = origin_w / origin_h;
                                let dx = point.x - anchor.x;
                                let dy = point.y - anchor.y;
                                if dx.abs() / dy.abs().max(0.001) > ratio {
                                    next.y = anchor.y + dy.signum() * dx.abs() / ratio;
                                } else {
                                    next.x = anchor.x + dx.signum() * dy.abs() * ratio;
                                }
                            }
                        }
                        *target = next;
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

    /// The pencil button in the layers list: switch to the selection tool and
    /// select the layer so its dashed frame and resize handles appear.
    pub fn image_editor_edit_layer(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(editor) = self.image_editor.as_mut() {
            editor.tool = AnnotationTool::Select;
            editor.selected = Some(index);
            editor.text_entry = None;
            editor.annotation_drag = None;
            cx.notify();
        }
    }

    /// Drag that starts on the dashed selection frame itself (not only on a
    /// stroke): moves the whole annotation.
    pub fn image_editor_begin_move(
        &mut self,
        index: usize,
        position: GPoint<gpui::Pixels>,
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
        editor.selected = Some(index);
        if let Some(annotation) = editor.edit.annotations.get(index) {
            let origin = annotation.points.clone();
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
}

fn preview_is_tiny(annotation: &Annotation) -> bool {
    let Some(bounds) = annotation.bounds() else {
        return true;
    };
    bounds.w < 6.0 && bounds.h < 6.0
}
