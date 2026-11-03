use super::*;

impl AppView {
    pub(crate) fn table_lines_slice(doc: &crate::document_buffer::DocumentBuffer, block: &crate::table::TableBlock) -> Vec<String> {
        (block.first_line..=block.last_line)
            .map(|i| doc.line_text(i))
            .collect()
    }

    pub(crate) fn get_or_compute_table_layout<'a>(
        table_layouts: &std::cell::RefCell<std::collections::HashMap<usize, (u64, Arc<TableLayout>)>>,
        text_system: &gpui::WindowTextSystem,
        get_line: &dyn Fn(usize) -> Option<&'a str>,
        block: &crate::table::TableBlock,
        font_family: &SharedString,
        manual_widths: Option<&[f32]>,
        char_width: f32,
        room_px: f32,
    ) -> Arc<TableLayout> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for l_idx in block.first_line..=block.last_line {
            if let Some(l) = get_line(l_idx) {
                l.hash(&mut hasher);
            }
        }
        if let Some(mw) = manual_widths {
            for &w in mw {
                w.to_bits().hash(&mut hasher);
            }
        }
        // The wrap width and the cell width decide how the cells wrap, so a
        // resize of the pane or a font change must not reuse an old layout.
        char_width.to_bits().hash(&mut hasher);
        room_px.to_bits().hash(&mut hasher);
        let content_hash = hasher.finish();

        if let Ok(layouts) = table_layouts.try_borrow() {
            if let Some((h, layout)) = layouts.get(&block.first_line) {
                if *h == content_hash {
                    return layout.clone();
                }
            }
        }

        let layout = Arc::new(compute_table_layout(
            text_system,
            get_line,
            block,
            font_family,
            manual_widths,
            char_width,
            room_px,
        ));
        if let Ok(mut layouts) = table_layouts.try_borrow_mut() {
            layouts.insert(block.first_line, (content_hash, layout.clone()));
        }
        layout
    }

    /// Cached `shape_row` for ordinary (non-table) editor rows.
    ///
    /// The hash covers every input that changes the shaped result: the line
    /// text, its classification (drives the painted text and the font size),
    /// the row's char range, the active-line flag (block prefixes are painted
    /// only on the active line), the inline reveal (per-element markup), the
    /// font family and the cell width. A change to heading sizes invalidates
    /// via `clear_row_shape_cache`, called from the settings subscription.
    pub(crate) fn get_or_shape_row(
        row_cache: &std::cell::RefCell<HashMap<u64, Arc<RowShapedLayout>>>,
        text_system: &gpui::WindowTextSystem,
        line: &str,
        classified: &crate::markdown::LineKind<'_>,
        v: &crate::markdown::VisualRow,
        is_active_line: bool,
        reveal: &crate::markdown::InlineReveal,
        headings: &crate::settings::HeadingsSettings,
        font_family: &SharedString,
        cell_w: f32,
    ) -> Arc<RowShapedLayout> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        line.hash(&mut hasher);
        std::mem::discriminant(&LineKindTag::of(classified)).hash(&mut hasher);
        v.line_idx.hash(&mut hasher);
        v.sub_idx.hash(&mut hasher);
        v.char_start.hash(&mut hasher);
        v.char_end.hash(&mut hasher);
        is_active_line.hash(&mut hasher);
        reveal.hash(&mut hasher);
        font_family.hash(&mut hasher);
        cell_w.to_bits().hash(&mut hasher);
        headings_cache_key(headings).hash(&mut hasher);
        let key = hasher.finish();

        if let Ok(cache) = row_cache.try_borrow() {
            if let Some(layout) = cache.get(&key) {
                crate::perf::PERF
                    .shape_cache_hits
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return layout.clone();
            }
        }

        let layout = Arc::new(shape_row(text_system, line, classified, v, is_active_line, reveal, headings, font_family, cell_w));
        if let Ok(mut cache) = row_cache.try_borrow_mut() {
            // Bounded the same way a frame is: the visible row set. A large
            // vault scroll leaves stale entries behind; drop the whole cache
            // when it grows past a few frames' worth of rows.
            if cache.len() >= 4096 {
                cache.clear();
            }
            cache.insert(key, layout.clone());
        }
        layout
    }

    /// Figure numbers for every captioned image in the document. Rebuilt only
    /// when the text or an `.edit.json` changes; the scan is allocation-light
    /// and only called when a captioned image is actually on screen.
    pub(super) fn caption_numbers_for(
        &self,
        doc: &crate::document_buffer::DocumentBuffer,
    ) -> std::sync::Arc<std::collections::HashMap<usize, u32>> {
        let generation = crate::images::edit_generation();
        if let Some((doc_id, version, cached_generation, numbers)) =
            self.caption_numbers_cache.borrow().as_ref()
        {
            if *doc_id == doc.id && *version == doc.version && *cached_generation == generation {
                return std::sync::Arc::clone(numbers);
            }
        }
        let root = crate::images::vault_root();
        let folder = crate::images::folder();
        let mut numbers = std::collections::HashMap::new();
        let mut current = 0u32;
        for (line_idx, line) in doc.rope.lines().enumerate() {
            let mut chars = line.chars().skip_while(|c| c.is_whitespace());
            if chars.next() != Some('!') {
                continue;
            }
            if chars.next() != Some('[') || chars.next() != Some('[') {
                continue;
            }
            let text = line.to_string();
            let Some(reference) = crate::images::parse_image_reference(&text) else {
                continue;
            };
            let Some(path) = crate::images::resolve_attachment(
                root.as_deref(),
                &doc.path,
                &folder,
                &reference.target,
            ) else {
                continue;
            };
            let edit = crate::images::load_edit(&path);
            let Some(caption) = edit.caption else {
                continue;
            };
            if caption.text.trim().is_empty() || !caption.numbered {
                continue;
            }
            current += 1;
            numbers.insert(line_idx, current);
        }
        let numbers = std::sync::Arc::new(numbers);
        *self.caption_numbers_cache.borrow_mut() =
            Some((doc.id, doc.version, generation, std::sync::Arc::clone(&numbers)));
        numbers
    }

    pub(crate) fn select_image_line(&mut self, line: usize) {
        // Selecting a picture is an object selection: a leftover text
        // selection or anchor would keep a highlight and a caret around while
        // the picture owns the focus of attention.
        if let Some(view) = self.state.active_view_mut() {
            view.selection_anchor = None;
        }
        self.is_mouse_selecting = false;
        self.mouse_down_pos = None;
        self.selected_image_line = Some(line);
        self.selected_image_doc = self.state.active_doc_id();
    }

    pub(crate) fn clear_image_selection(&mut self) {
        self.selected_image_line = None;
        self.selected_image_doc = None;
    }

    pub(crate) fn attachment_clipboard_metadata(
        &self,
        text: &str,
        cut: bool,
    ) -> Option<serde_json::Value> {
        let trimmed = text.trim();
        if trimmed.lines().count() != 1 {
            return None;
        }
        let reference = crate::images::parse_image_reference(trimmed)?;
        if !reference.wiki {
            return None;
        }
        let doc = self.state.active_document()?;
        let root = crate::images::vault_root();
        crate::images::resolve_attachment(
            root.as_deref(),
            &doc.path,
            &self.config.files.folder,
            &reference.target,
        )?;
        Some(serde_json::json!({
            "vibecodiumAttachment": {
                "name": reference.target,
                "note": doc.path.to_string_lossy(),
                "cut": cut
            }
        }))
    }

    /// Line of the image block under a window-space Y, if any. Used by the
    /// context menu, which must not move the caret just to find its target.
    pub(crate) fn image_line_at_window_pos(&self, y: f32) -> Option<usize> {
        let view = self.state.active_view()?;
        if view.window.is_empty() {
            return None;
        }
        let rel_y = f64::from(y) - f64::from(self.editor_rows_top_y())
            + self.current_scroll_top_f64();
        if rel_y < 0.0 {
            return None;
        }
        let v_idx = view.local_v_idx_at_doc_y(rel_y);
        let v = view.window.visual_rows.get(v_idx)?;
        let text = view.window.lines.get(v.line_idx)?;
        crate::images::parse_image_reference(text)?;
        Some(v.line_idx)
    }
}
