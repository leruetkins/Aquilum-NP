use super::*;

impl AppView {
    pub fn save_image_as(&mut self, line: usize, cx: &mut Context<Self>) {
        let Some(doc) = self.state.active_document() else {
            return;
        };
        let text = doc.line_text(line);
        let wrap_cols = self.state.active_view().map(|view| view.window.wrap_cols).unwrap_or(80);
        let Some(image) = images::resolve_for_note(&doc.path, &text, wrap_cols, false) else {
            return;
        };
        let edit = image.edit.clone();
        let caption = edit
            .caption
            .as_ref()
            .filter(|caption| !caption.text.trim().is_empty())
            .map(|caption| (caption.text.clone(), caption.position.clone()));
        let options = crate::image_render::BakeOptions {
            apply_crop: true,
            apply_corner_radius: true,
            caption,
            caption_background: None,
        };
        let baked = match crate::image_render::bake_rgba(&image.path, &edit, &options)
            .and_then(|rgba| crate::image_render::encode_rgba(&rgba, crate::image_render::EncodedFormat::Png, 92))
        {
            Ok(bytes) => bytes,
            Err(error) => {
                self.push_notification(NotificationType::Error, None, error, Some(5000), cx);
                return;
            }
        };
        let default_name = format!(
            "{}-edited.png",
            image.path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()
        );
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        cx.foreground_executor()
            .spawn(async move {
                let handle = rfd::AsyncFileDialog::new()
                    .add_filter("PNG image (*.png)", &["png"])
                    .set_file_name(&default_name)
                    .save_file()
                    .await;
                if let Some(handle) = handle {
                    let path = handle.path().to_path_buf();
                    let _ = entity.update(&mut async_cx, |this, cx| {
                        match std::fs::write(&path, &baked) {
                            Ok(()) => {
                                this.push_notification(
                                    NotificationType::Success,
                                    None,
                                    crate::i18n::t("editor.image.saved_image", &this.config.ui.language),
                                    Some(4000),
                                    cx,
                                );
                            }
                            Err(_) => {
                                this.push_notification(
                                    NotificationType::Error,
                                    None,
                                    crate::i18n::t("editor.image.save_failed", &this.config.ui.language),
                                    Some(5000),
                                    cx,
                                );
                            }
                        }
                        cx.notify();
                    });
                }
            })
            .detach();
    }

    pub fn open_image_context_menu(&mut self, line: usize, x: f32, y: f32, cx: &mut Context<Self>) {
        use crate::views::editor_context_menu::{item, separator, ContextMenuState};
        let lang = self.config.ui.language.as_str();
        let t = |key: &str| crate::i18n::t(key, lang);
        let items = vec![
            item("image-copy-reference", t("editor.image.menu_copy_ref"), "copy", false),
            item("image-cut", t("editor.image.menu_cut"), "cut", false),
            item("image-paste", t("editor.image.menu_paste"), "download", false),
            separator("sep-image-clipboard"),
            item("image-copy", t("editor.image.menu_copy"), "copy", false),
            item("image-copy-link", t("editor.image.menu_copy_link"), "link", false),
            item("image-save-as", t("editor.image.menu_save_as"), "download", false),
            separator("sep-image-clipboard-2"),
            item("image-size", t("editor.image.menu_size"), "move", false),
            item("image-radius", t("editor.image.menu_radius"), "maximize", false),
            separator("sep-image-edit"),
            item("image-crop", t("editor.image.menu_crop"), "crop", false),
            item("image-annotate", t("editor.image.menu_annotate"), "pencil", false),
            item("image-caption", t("editor.image.menu_caption"), "type", false),
            item("image-compress", t("editor.image.menu_compress"), "download", false),
            separator("sep-image-destructive"),
            item("image-open-original", t("editor.image.menu_show_original"), "eye", false),
            item("image-delete", t("editor.image.menu_delete"), "trash", false),
        ];
        let session_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.image_popover = None;
        self.editor_context_menu = ContextMenuState {
            is_open: true,
            session_id,
            position_x: x,
            position_y: y,
            items,
            active_path: Vec::new(),
            table_target: None,
            image_target: Some(line),
        };
        self.select_image_line(line);
        cx.notify();
    }

    /// Puts the attachment's own bytes back on the clipboard as an image.
    pub fn copy_image_to_clipboard(&self, line: usize, cx: &mut Context<Self>) {
        let Some(doc) = self.state.active_document() else {
            return;
        };
        let root = images::vault_root();
        let folder = self.config.files.folder.clone();
        let text = doc.line_text(line);
        let Some(reference) = images::parse_image_reference(&text) else {
            return;
        };
        let Some(path) = images::resolve_attachment(root.as_deref(), &doc.path, &folder, &reference.target) else {
            return;
        };
        let Some(format) = std::path::Path::new(&reference.target)
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext.to_ascii_lowercase().as_str() {
                "png" => Some(gpui::ImageFormat::Png),
                "jpg" | "jpeg" => Some(gpui::ImageFormat::Jpeg),
                "webp" => Some(gpui::ImageFormat::Webp),
                "gif" => Some(gpui::ImageFormat::Gif),
                "bmp" => Some(gpui::ImageFormat::Bmp),
                "tif" | "tiff" => Some(gpui::ImageFormat::Tiff),
                _ => None,
            })
        else {
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_image(&gpui::Image::from_bytes(
            format, bytes,
        )));
    }

    /// Copies the wiki reference line itself, with metadata that lets a paste
    /// into another note copy (or move, for Cut) the image and its sidecar.
    pub fn copy_image_reference(
        &mut self,
        line: usize,
        cut: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(doc) = self.state.active_document() else {
            return false;
        };
        let text = doc.line_text(line);
        let Some(reference) = images::parse_image_reference(&text) else {
            return false;
        };
        if !reference.wiki {
            return false;
        }
        let Some(meta) = self.attachment_clipboard_metadata(&text, cut) else {
            return false;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string_with_json_metadata(
            text.clone(),
            meta,
        ));
        crate::images::set_clipboard_attachment_meta(&doc.path, &reference.target, cut);
        if cut {
            images::protect_cut(&doc.path, &reference.target);
            self.delete_image_at_line(line, cx);
        }
        true
    }

    /// Handles every clipboard payload that carries a picture: a raster, a
    /// file list, or wiki references (with or without attachment metadata).
    /// Returns `true` when the paste was consumed, so callers may fall back to
    /// plain text only for text-only clipboards.
    pub fn paste_clipboard_image_aware(&mut self, cx: &mut Context<Self>) -> bool {
        let raster = cx
            .read_from_clipboard()
            .and_then(|item| {
                item.entries().iter().find_map(|entry| match entry {
                    gpui::ClipboardEntry::Image(image) => {
                        Some((image.bytes().to_vec(), image.format()))
                    }
                    _ => None,
                })
            })
            .or_else(crate::images::clipboard_raster);
        if let Some((bytes, format)) = raster {
            if self.import_clipboard_image(bytes, format, cx) {
                return true;
            }
        }
        if let Some(paths) = crate::images::clipboard_file_paths() {
            if self.import_clipboard_files(paths, cx) {
                return true;
            }
        }
        if let Some(item) = cx.read_from_clipboard() {
            if let Some(text) = item.text() {
                if let Some(meta) = crate::images::clipboard_attachment_meta() {
                    if self.copy_pasted_references(&text, Some(&meta), cx) {
                        return true;
                    }
                }
                if let Some(meta) = item
                    .metadata()
                    .and_then(|meta| serde_json::from_str::<serde_json::Value>(meta).ok())
                    .and_then(|value| value.get("vibecodiumAttachment").cloned())
                {
                    if self.copy_pasted_references(&text, Some(&meta), cx) {
                        return true;
                    }
                }
                if text.contains("![[") && self.copy_pasted_references(&text, None, cx) {
                    return true;
                }
            }
        }
        false
    }

    /// Context-menu Paste: inserts the clipboard right after the image line.
    pub fn paste_after_image(&mut self, line: usize, cx: &mut Context<Self>) {
        if let Some((view, doc)) = self.state.active_view_and_doc_mut() {
            let offset = doc.line_end_byte(line);
            view.cursor_offset = offset;
            view.selection_anchor = None;
            view.visual_cache_dirty = true;
        }
        if self.paste_clipboard_image_aware(cx) {
            return;
        }
        if let Some(item) = cx.read_from_clipboard() {
            if let Some(text) = item.text() {
                let text = crate::document_buffer::normalize_newlines(&text).into_owned();
                self.state.active_insert_text(&text);
                self.schedule_auto_save(cx);
                cx.notify();
            }
        }
    }

    /// Image actions from keyboard shortcuts when the picture is selected.
    pub fn handle_selected_image_clipboard_key(
        &mut self,
        key: &str,
        ctrl: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !ctrl {
            return false;
        }
        let Some(line) = self.selected_image_line else {
            return false;
        };
        if self.selected_image_doc != self.state.active_doc_id() {
            return false;
        }
        if key.eq_ignore_ascii_case("c") || key == "с" || key == "С" {
            return self.copy_image_reference(line, false, cx);
        }
        if key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч" {
            return self.copy_image_reference(line, true, cx);
        }
        false
    }

    pub fn copy_image_link(&self, line: usize, cx: &mut Context<Self>) {
        let Some(doc) = self.state.active_document() else {
            return;
        };
        let text = doc.line_text(line).trim().to_string();
        if !text.is_empty() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
    }

    /// Context-menu entry point for the size/radius editors: resolves the line
    /// and anchors the popover at the menu position.
    pub fn open_image_popover_at_line(
        &mut self,
        kind: ImagePopoverKind,
        line: usize,
        x: f32,
        y: f32,
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
        self.open_image_popover(kind, &image, line, x, y, window, cx);
    }

    /// "Insert image…" command: native picker, then the same importer as a
    /// drop or a clipboard paste.
    pub fn open_image_file_dialog(&mut self, cx: &mut Context<Self>) {
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let start = self
            .state
            .active_document()
            .and_then(|doc| doc.path.parent().map(|parent| parent.to_path_buf()))
            .or_else(|| self.state.workspace_path.clone());
        cx.foreground_executor()
            .spawn(async move {
                let mut dialog = rfd::AsyncFileDialog::new()
                    .add_filter("Images", images::IMAGE_EXTENSIONS);
                if let Some(dir) = start {
                    dialog = dialog.set_directory(dir);
                }
                if let Some(handles) = dialog.pick_files().await {
                    let paths: Vec<PathBuf> =
                        handles.into_iter().map(|handle| handle.path().to_path_buf()).collect();
                    let _ = entity.update(&mut async_cx, |this, cx| {
                        this.import_image_files(paths, cx);
                    });
                }
            })
            .detach();
    }

    // ─────────────────────── Import ───────────────────────

    /// Imports raster clipboard data and inserts the reference at the caret.
    /// Returns `false` when there is no active note or nothing was written.
    pub fn import_clipboard_image(
        &mut self,
        bytes: Vec<u8>,
        format: gpui::ImageFormat,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(note) = self.state.active_document().map(|doc| doc.path.clone()) else {
            return false;
        };
        let folder = self.config.files.folder.clone();
        let results = images::import_images(
            &note,
            &folder,
            ImageImportSource::Clipboard { bytes, format },
        );
        self.finish_image_import(results, cx)
    }

    /// Clipboard file list (Explorer's Ctrl+C). Image files are imported;
    /// anything else is skipped with one toast. Returns true when the
    /// clipboard *was* a file list, so the caller does not paste its merged
    /// text representation.
    pub fn import_clipboard_files(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) -> bool {
        if paths.is_empty() {
            return false;
        }
        let images: Vec<PathBuf> = paths
            .iter()
            .filter(|path| images::is_image_path(path))
            .cloned()
            .collect();
        let skipped = paths.len().saturating_sub(images.len());
        if !images.is_empty() {
            self.import_image_files(images, cx);
        }
        if skipped > 0 {
            let language = self.config.ui.language.clone();
            self.push_notification(
                NotificationType::Info,
                None,
                crate::i18n::t("editor.image.drop_not_image", &language),
                Some(5000),
                cx,
            );
        }
        true
    }

    /// Imports files (drop, clipboard file list, picker) next to the note.
    pub fn import_image_files(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) -> bool {
        if paths.is_empty() {
            return false;
        }
        let Some(note) = self.state.active_document().map(|doc| doc.path.clone()) else {
            return false;
        };
        let folder = self.config.files.folder.clone();
        for path in &paths {
            if let Ok(metadata) = std::fs::metadata(path) {
                if metadata.len() > 20 * 1024 * 1024 {
                    self.push_notification(
                        NotificationType::Info,
                        None,
                        crate::i18n::t("editor.image.large_file", &self.config.ui.language),
                        Some(6000),
                        cx,
                    );
                    break;
                }
            }
        }
        let results = images::import_images(&note, &folder, ImageImportSource::Files(paths));
        self.finish_image_import(results, cx)
    }

    pub(super) fn finish_image_import(
        &mut self,
        results: Vec<Result<ImportedImage, images::ImportError>>,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut imported = Vec::new();
        let mut first_error = None;
        for result in results {
            match result {
                Ok(image) => imported.push(image),
                Err(err) => {
                    if first_error.is_none() {
                        first_error = Some(err.message(&self.config.ui.language));
                    }
                }
            }
        }
        if let Some(message) = first_error {
            let language = self.config.ui.language.clone();
            self.push_notification(
                NotificationType::Error,
                Some(crate::i18n::t("editor.image.import_error", &language)),
                message,
                Some(6000),
                cx,
            );
        }
        if imported.is_empty() {
            return false;
        }

        let references: Vec<String> = imported
            .iter()
            .map(|image| format!("![[{}]]", image.name))
            .collect();
        let text = self.image_insert_text(&references);
        self.state.active_replace_selection(&text);
        self.schedule_auto_save(cx);
        // Put the caret on the empty line created under the last inserted
        // reference and select that reference line, so the picture is painted
        // at once instead of its markup.
        let last_reference = references.last().cloned().unwrap_or_default();
        let reference_count = references.len();
        let inserted_line = self.state.active_view_and_doc_mut().and_then(|(view, doc)| {
            let lines_total = doc.line_index.lines();
            let (caret_line, _) = view.line_col(doc);
            let first = caret_line.saturating_sub(reference_count + 3);
            let mut ref_line = None;
            for line in first..=caret_line.min(lines_total.saturating_sub(1)) {
                if doc.line_text(line).trim() == last_reference {
                    ref_line = Some(line);
                }
            }
            let ref_line = ref_line?;
            let empty_line = (ref_line + 1).min(lines_total.saturating_sub(1));
            let offset = doc.line_start_byte(empty_line);
            view.cursor_offset = offset;
            view.selection_anchor = None;
            view.visual_cache_dirty = true;
            Some(ref_line)
        });
        if let Some(line) = inserted_line {
            self.need_reveal_cursor = true;
            self.select_image_line(line);
            self.hovered_image_line = Some(line);
        }
        if self.config.images.open_editor_after_insert {
            let info = inserted_line.and_then(|line| {
                self.state.active_view_and_doc().map(|(view, doc)| {
                    (
                        line,
                        doc.path.clone(),
                        doc.line_text(line),
                        view.window.wrap_cols,
                    )
                })
            });
            if let Some((line, note, text, wrap_cols)) = info {
                if let Some(image) = images::resolve_for_note(&note, &text, wrap_cols, false) {
                    self.open_image_editor(
                        crate::views::image_editor::ImageEditorMode::Crop,
                        line,
                        &image,
                        cx,
                    );
                }
            }
        }
        self.editor_blink
            .pause_and_show(crate::blink_manager::BlinkTarget::Editor, cx);
        cx.notify();
        true
    }

    /// Wraps the references in line breaks so each one owns a block line and
    /// the rest of the current line is not glued to the last reference.
    pub(super) fn image_insert_text(&self, references: &[String]) -> String {
        let Some((view, doc)) = self.state.active_view_and_doc() else {
            return references.join("\n");
        };
        let (line, col) = view.line_col(doc);
        let line_start = doc.line_start_byte(line);
        let line_end = doc.line_end_byte(line);
        let text = doc.slice_to_string(line_start..line_end);
        let byte_col = text
            .char_indices()
            .nth(col)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len());
        let before = &text[..byte_col];
        let after = &text[byte_col..];
        let mut out = String::new();
        if !before.trim().is_empty() {
            out.push('\n');
        }
        out.push_str(&references.join("\n"));
        // One break ends the reference line, the second creates the empty
        // line that the caret will occupy under the picture.
        out.push_str("\n\n");
        let _ = after;
        out
    }

    /// Detects a clipboard payload made only of paths to image files.
    pub fn clipboard_image_files(text: &str) -> Option<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for line in text.lines() {
            let candidate = line.trim().trim_matches('"');
            if candidate.is_empty() {
                continue;
            }
            let path = Path::new(candidate);
            if !path.is_file() || !images::is_image_path(path) {
                return None;
            }
            paths.push(path.to_path_buf());
        }
        (!paths.is_empty()).then_some(paths)
    }

    /// Copies attachments referenced by pasted markup into this note under
    /// fresh names, so a copy never aliases the source note's file.
    pub fn copy_pasted_references(
        &mut self,
        text: &str,
        meta: Option<&serde_json::Value>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(note) = self.state.active_document().map(|doc| doc.path.clone()) else {
            return false;
        };
        let folder = self.config.files.folder.clone();
        let root = images::vault_root();
        let cut = meta
            .and_then(|meta| meta.get("cut"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let source_note = meta
            .and_then(|meta| meta.get("note"))
            .and_then(|value| value.as_str())
            .map(std::path::PathBuf::from);
        let source_name = meta
            .and_then(|meta| meta.get("name"))
            .and_then(|value| value.as_str());
        let mut rewritten = Vec::new();
        let mut any = false;
        let mut moved = false;
        for line in text.lines() {
            let Some(reference) = images::parse_image_reference(line.trim()) else {
                return false;
            };
            if !reference.wiki {
                return false;
            }
            let resolve_note = source_note.as_deref().unwrap_or(&note);
            if images::resolve_attachment(root.as_deref(), resolve_note, &folder, &reference.target).is_none() {
                return false;
            }
            any = true;
            let can_move = cut
                && source_note.is_some()
                && source_name == Some(reference.target.as_str());
            if cut {
                images::release_cut(
                    source_note.as_deref().unwrap_or(&note),
                    &reference.target,
                );
            }
            let replacement = if can_move {
                match images::move_attachment(
                    root.as_deref(),
                    source_note.as_deref().unwrap_or(&note),
                    &note,
                    &folder,
                    &reference.target,
                ) {
                    Some(name) => {
                        moved = true;
                        name
                    }
                    None => images::copy_attachment_to(
                        root.as_deref(),
                        source_note.as_deref().unwrap_or(&note),
                        &note,
                        &folder,
                        &reference.target,
                    )
                    .unwrap_or_else(|| reference.target.clone()),
                }
            } else {
                images::copy_attachment_to(
                    root.as_deref(),
                    resolve_note,
                    &note,
                    &folder,
                    &reference.target,
                )
                .unwrap_or_else(|| reference.target.clone())
            };
            rewritten.push(reference.with_target(replacement).markup());
        }
        if !any {
            return false;
        }
        let text = self.image_insert_text(&rewritten);
        self.state.active_replace_selection(&text);
        if moved {
            let meta = serde_json::json!({
                "vibecodiumAttachment": {
                    "name": "",
                    "note": note.to_string_lossy(),
                    "cut": false
                }
            });
            cx.write_to_clipboard(gpui::ClipboardItem::new_string_with_json_metadata(text.clone(), meta));
            crate::images::set_clipboard_attachment_meta(&note, "", false);
        }
        self.schedule_auto_save(cx);
        cx.notify();
        true
    }



    pub fn open_image_popover(
        &mut self,
        kind: ImagePopoverKind,
        image: &ResolvedImage,
        line: usize,
        x: f32,
        y: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut width = TextInput::new(cx.focus_handle());
        let mut height = TextInput::new(cx.focus_handle());
        let displayed_w = image.width.round().max(1.0) as u32;
        let displayed_h = image.height.round().max(1.0) as u32;
        width.set_text(image.reference.width.unwrap_or(displayed_w).to_string());
        height.set_text(image.reference.height.unwrap_or(displayed_h).to_string());
        let focus = cx.focus_handle();
        if kind == ImagePopoverKind::Size {
            if let Some(handle) = width.focus_handle.clone() {
                window.focus(&handle);
            }
        } else {
            window.focus(&focus);
        }
        self.editor_context_menu.is_open = false;
        self.select_image_line(line);
        self.image_popover = Some(ImagePopoverState {
            kind,
            line,
            path: image.path.clone(),
            reference: image.reference.clone(),
            natural: image.natural,
            visible: image.visible,
            wrap_cols: self.state.active_view().map(|view| view.window.wrap_cols).unwrap_or(80),
            x,
            y,
            focus,
            width,
            height,
            active: SizeField::Width,
            lock: true,
        });
        cx.notify();
    }

    pub fn image_size_drag_move(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        _shift: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(mut drag) = self.size_drag.take() else {
            return;
        };
        let dx = f32::from(position.x) - drag.start.0;
        let dy = f32::from(position.y) - drag.start.1;
        let (start_w, start_h) = drag.start_size;
        let mut width = start_w + drag.sign.0 * dx;
        let mut height = start_h + drag.sign.1 * dy;
        if drag.sign.0 == 0.0 {
            width = start_w;
        }
        if drag.sign.1 == 0.0 {
            height = start_h;
        }
        width = width.clamp(16.0, drag.max_width.max(16.0));
        height = height.clamp(16.0, 4000.0);
        if drag.lock && start_w > 0.0 && start_h > 0.0 {
            if drag.sign.0 != 0.0 {
                height = (width * start_h / start_w).clamp(16.0, 4000.0);
            } else {
                width = (height * start_w / start_h).clamp(16.0, drag.max_width.max(16.0));
            }
        }
        drag.current = (width.round(), height.round());
        self.size_drag = Some(drag);
        cx.notify();
    }

    pub fn image_size_drag_end(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.size_drag.take() else {
            return;
        };
        let width = drag.current.0.round().max(16.0) as u32;
        let height = drag.current.1.round().max(16.0) as u32;
        let popover_open = self
            .image_popover
            .as_ref()
            .is_some_and(|popover| popover.line == drag.line && popover.kind == ImagePopoverKind::Size);
        if popover_open {
            if let Some(popover) = self.image_popover.as_mut() {
                popover.width.set_text(width.to_string());
                popover.height.set_text(height.to_string());
                popover.lock = drag.lock;
            }
        }
        let reference = self
            .state
            .active_document()
            .map(|doc| doc.line_text(drag.line))
            .and_then(|text| images::parse_image_reference(&text));
        if let Some(reference) = reference {
            let updated = if drag.lock {
                reference.with_size(Some(width), None)
            } else {
                reference.with_size(Some(width), Some(height))
            };
            self.replace_image_reference(drag.line, &updated.markup(), cx);
            if popover_open {
                if let Some(popover) = self.image_popover.as_mut() {
                    popover.reference = updated;
                }
            }
        }
        cx.notify();
    }

    pub fn close_image_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.size_drag = None;
        if self.image_popover.take().is_some() {
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    /// Rewrites the reference on the popover's line. The caret is kept where it
    /// was: rewriting a line must not activate it (which would hide the image).
    pub fn apply_image_size(&mut self, cx: &mut Context<Self>) {
        let Some(popover) = self.image_popover.as_ref() else {
            return;
        };
        let line = popover.line;
        let name = popover.reference.target.clone();
        let raw_width = popover.width.text().trim().parse::<u32>().ok();
        let raw_height = popover.height.text().trim().parse::<u32>().ok();
        let (width, height) = if popover.lock {
            locked_display_size(popover.active, raw_width, raw_height, popover.visible)
        } else {
            (raw_width, raw_height)
        };
        let mut reference = popover.reference.clone();
        reference.target = name;
        reference = reference.with_size(width, height);
        self.replace_image_reference(line, &reference.markup(), cx);
        if let Some(popover) = self.image_popover.as_mut() {
            popover.reference = reference;
        }
    }

    /// Applies the "Reset size" action: the reference keeps only the name.
    pub fn reset_image_size(&mut self, cx: &mut Context<Self>) {
        let Some(popover) = self.image_popover.as_ref() else {
            return;
        };
        let line = popover.line;
        let reference = popover.reference.with_size(None, None);
        self.replace_image_reference(line, &reference.markup(), cx);
        if let Some(popover) = self.image_popover.as_mut() {
            let max_width = images::text_width(popover.wrap_cols);
            let displayed_w = (popover.reference.display_size(popover.natural, max_width).0)
                .round()
                .max(1.0) as u32;
            let displayed_h = (popover.reference.display_size(popover.natural, max_width).1)
                .round()
                .max(1.0) as u32;
            popover.width.set_text(displayed_w.to_string());
            popover.height.set_text(displayed_h.to_string());
            popover.reference = reference;
        }
    }

    pub fn set_image_size_percent(&mut self, percent: u32, cx: &mut Context<Self>) {
        let Some(popover) = self.image_popover.as_ref() else {
            return;
        };
        let width = ((popover.visible.0 as f32) * percent as f32 / 100.0).round().max(1.0) as u32;
        let height = ((popover.visible.1 as f32) * percent as f32 / 100.0).round().max(1.0) as u32;
        if let Some(popover) = self.image_popover.as_mut() {
            popover.width.set_text(width.to_string());
            popover.height.set_text(height.to_string());
        }
        self.apply_image_size(cx);
    }

    pub fn set_image_radius(&mut self, percent: f32, cx: &mut Context<Self>) {
        let Some(popover) = self.image_popover.as_ref() else {
            return;
        };
        let path = popover.path.clone();
        if crate::images::set_corner_radius(&path, Some(percent)).is_ok() {
            cx.notify();
        }
    }

    pub fn reset_image_radius(&mut self, cx: &mut Context<Self>) {
        let Some(popover) = self.image_popover.as_ref() else {
            return;
        };
        let path = popover.path.clone();
        if crate::images::set_corner_radius(&path, None).is_ok() {
            cx.notify();
        }
    }

    pub(crate) fn replace_image_reference(&mut self, line: usize, markup: &str, cx: &mut Context<Self>) {
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        let start = doc.line_start_byte(line);
        let end = doc.line_end_byte(line);
        if start >= doc.len() {
            return;
        }
        let old_cursor = view.cursor_offset.min(doc.len());
        let old_len = end.saturating_sub(start);
        let new_len = markup.len();
        let new_cursor = if old_cursor > end {
            old_cursor + new_len - old_len
        } else if old_cursor >= start {
            start + new_len
        } else {
            old_cursor
        };
        let (_, note_edit, edit_event) = view.replace_range(doc, start..end, markup.to_string(), new_cursor);
        self.state.dispatch_edit_event(&edit_event, &note_edit);
        self.schedule_auto_save(cx);
        cx.notify();
    }

    pub fn delete_image_at_line(&mut self, line: usize, cx: &mut Context<Self>) {
        self.selected_image_line = None;
        self.hovered_image_line = None;
        self.image_popover = None;
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        let lines = doc.line_index.lines();
        let start = doc.line_start_byte(line);
        let end = if line + 1 < lines {
            doc.line_start_byte(line + 1)
        } else {
            doc.line_end_byte(line)
        };
        if start >= end {
            return;
        }
        let old_cursor = view.cursor_offset.min(doc.len());
        // Deleting a whole line is before, inside, or after the caret. Only the
        // "after" case may shift left by the removed length; a caret inside the
        // line must collapse to its start, otherwise the shifted byte offset
        // can land inside a multibyte character and panic in the rope.
        let new_cursor = if old_cursor >= end {
            old_cursor - (end - start)
        } else if old_cursor > start {
            start
        } else {
            old_cursor
        };
        let (_, note_edit, edit_event) = view.replace_range(doc, start..end, String::new(), new_cursor);
        self.state.dispatch_edit_event(&edit_event, &note_edit);
        self.schedule_auto_save(cx);
        cx.notify();
    }

    // ─────────────────────── Popover ───────────────────────
}
