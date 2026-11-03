use super::*;

impl AppView {
    /// Opens the in-editor find bar and focuses the query field.
    ///
    /// Calling it again while the bar is open only refocuses the field and
    /// selects the existing query, like pressing Ctrl+F twice.
    pub(crate) fn open_find_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.active_document().is_none() {
            return;
        }
        let already_open = self.find_bar.is_open;
        self.find_bar.is_open = true;
        if already_open {
            self.find_bar.query.select_all();
            if self.find_is_stale() && !self.find_request_is_current() {
                self.request_find_search(cx);
            }
        } else {
            // A word (or a part of one) selected in the editor becomes the
            // query: Ctrl+F right after selecting should search for it. A
            // selection spanning lines is not a query, so the old one stays.
            if let Some(selected) = self.state.active_selected_text() {
                let selected = selected.trim();
                if !selected.is_empty() && !selected.contains(['\n', '\r']) {
                    self.find_bar.query.set_text(selected);
                }
            }
            self.request_find_search(cx);
        }
        self.find_bar.replace.blink.disable(cx);
        self.find_bar.query.focus(window);
        self.find_bar.query.blink.pause_and_show(BlinkTarget::FindQuery, cx);
        cx.notify();
    }

    /// Closes the find bar, clears both fields and returns focus to the editor.
    pub(crate) fn close_find_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find_bar.is_open {
            return;
        }
        self.find_bar.is_open = false;
        // The expander's state is a preference, not a per-pass value: the next
        // Ctrl+F opens the bar the way the user left it.
        self.find_bar.matches.clear();
        self.find_bar.matches_query.clear();
        self.find_bar.matches_version = 0;
        self.find_bar.clear_marks();
        self.find_bar.active = 0;
        self.find_bar.truncated = false;
        // A reply from the scan still running must not repopulate the list.
        self.find_bar.search_seq = self.find_bar.search_seq.wrapping_add(1);
        self.find_bar
            .cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.find_bar.search_queued = false;
        self.find_bar.search_query.clear();
        self.find_bar.search_version = 0;
        self.find_bar.pending_step = None;
        self.find_bar.pending_after = None;
        self.find_bar.pending_replace = false;
        self.find_bar.pending_mark_all = false;
        // A Replace All pass still building text must not land after the bar
        // is closed.
        self.find_bar.cancel_replace_all();
        self.find_bar.query.clear();
        self.find_bar.replace.clear();
        self.find_bar.query.blink.disable(cx);
        self.find_bar.replace.blink.disable(cx);
        window.focus(&self.focus_handle);
        self.reset_editor_cursor_blink(cx);
        cx.notify();
    }

    /// Asks for a fresh match list without blocking the UI thread.
    ///
    /// The scan starts immediately; the next keystroke cancels it, and only one
    /// scan is ever in flight (a newer request is queued behind the running
    /// one). A debounce would add a fixed pause before any work at all, which
    /// is exactly what makes typing in a large document feel laggy. The
    /// previous list stays on screen until the reply arrives, so highlights do
    /// not blink on every keystroke.
    pub(super) fn request_find_search(&mut self, cx: &mut Context<Self>) {
        self.find_bar.search_seq = self.find_bar.search_seq.wrapping_add(1);
        let seq = self.find_bar.search_seq;
        self.find_bar
            .cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        // The arrows describe the lines of the previous result; a new search
        // means a new question, so they go away with it.
        self.find_bar.clear_marks();
        self.find_bar.search_query = self.find_bar.query.text().to_string();
        self.find_bar.search_options = self.find_bar.options();
        self.find_bar.search_version = self
            .state
            .active_document()
            .map(|doc| doc.version)
            .unwrap_or(0);
        if self.find_bar.query.is_empty() {
            self.find_bar.matches.clear();
            self.find_bar.matches_query.clear();
            self.find_bar.matches_options = self.find_bar.options();
            self.find_bar.matches_version = 0;
            self.find_bar.active = 0;
            self.find_bar.truncated = false;
            self.find_bar.pending_step = None;
            self.find_bar.pending_after = None;
            self.find_bar.pending_replace = false;
            self.find_bar.pending_mark_all = false;
            cx.notify();
            return;
        }
        self.start_find_scan(seq, cx);
        cx.notify();
    }

    /// Starts the background scan for `seq`, or queues it behind the one
    /// already running so only one full-document pass is in flight.
    pub(super) fn start_find_scan(&mut self, seq: u64, cx: &mut Context<Self>) {
        if self.find_bar.search_running {
            self.find_bar.search_queued = true;
            return;
        }
        let query = self.find_bar.query.text().to_string();
        let options = self.find_bar.options();
        let Some(doc) = self.state.active_document() else { return; };
        let version = doc.version;
        let rope = doc.rope().clone();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.find_bar.cancel = std::sync::Arc::clone(&cancel);
        self.find_bar.search_running = true;

        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let background = cx.background_executor().clone();
        cx.foreground_executor()
            .spawn(async move {
                let (query, options, matches) = background
                    .spawn(async move {
                        // The rope is immutable while the scan runs (the
                        // version guard in `finish_find_scan` rejects results
                        // from a document edited meanwhile), so it is scanned
                        // chunk by chunk instead of being copied into one big
                        // String.
                        let matches = crate::views::find_bar::scan_rope(
                            &rope,
                            &query,
                            options,
                            crate::views::find_bar::MAX_FIND_MATCHES,
                            &|| cancel.load(std::sync::atomic::Ordering::Relaxed),
                        );
                        (query, options, matches)
                    })
                    .await;
                let _ = entity.update(&mut async_cx, |this, cx| {
                    this.finish_find_scan(seq, version, query, options, matches, cx);
                });
            })
            .detach();
    }

    /// Installs a background result, then runs the newest queued request.
    pub(super) fn finish_find_scan(
        &mut self,
        seq: u64,
        version: u64,
        query: String,
        options: crate::views::find_bar::FindOptions,
        matches: Vec<std::ops::Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        self.find_bar.search_running = false;
        // Ranges are only meaningful for the exact document revision the
        // scan read; an edit during the scan throws this reply away.
        let document_unchanged = self
            .state
            .active_document()
            .map(|doc| doc.version == version)
            .unwrap_or(false);
        let newest = seq == self.find_bar.search_seq && self.find_bar.is_open;
        if newest && document_unchanged {
            self.find_bar.truncated =
                matches.len() >= crate::views::find_bar::MAX_FIND_MATCHES;
            self.find_bar.matches = matches;
            self.find_bar.matches_query = query;
            self.find_bar.matches_options = options;
            self.find_bar.matches_version = version;
            self.find_bar.active = 0;
            let step = self.find_bar.pending_step.take();
            let after = self.find_bar.pending_after.take();
            // A replace pressed while the list was stale waits here for the
            // fresh ranges instead of being dropped.
            let pending_replace = self.find_bar.pending_replace;
            self.find_bar.pending_replace = false;
            // "Mark all" pressed on a stale list marks these ranges instead of
            // the ones the old list described.
            let pending_mark_all = self.find_bar.pending_mark_all;
            self.find_bar.pending_mark_all = false;
            // A query-only refresh shows the match without moving the view;
            // Enter or a replacement asked for the position explicitly.
            let reveal = step.is_some() || after.is_some();
            if !self.find_bar.matches.is_empty() {
                let index = if let Some(delta) = step {
                    // The list is brand new: "next" is its first match and
                    // "previous" its last, exactly like a fresh search.
                    if delta >= 0 {
                        0
                    } else {
                        self.find_bar.matches.len() - 1
                    }
                } else if let Some(offset) = after {
                    self.find_bar
                        .matches
                        .iter()
                        .position(|m| m.start >= offset)
                        .unwrap_or(0)
                } else {
                    0
                };
                self.find_activate(index, reveal, cx);
                if pending_mark_all {
                    self.find_mark_all(cx);
                }
                if pending_replace {
                    self.find_replace_current(cx);
                }
            } else {
                cx.notify();
            }
        } else if newest {
            // Offsets and ranges from that revision no longer describe the
            // current text, so a queued navigation, marking or replacement is
            // dropped instead of being applied later to something it was not
            // meant for.
            self.find_bar.pending_step = None;
            self.find_bar.pending_after = None;
            self.find_bar.pending_replace = false;
            self.find_bar.pending_mark_all = false;
        }
        if self.find_bar.search_queued {
            self.find_bar.search_queued = false;
            let seq = self.find_bar.search_seq;
            if self.find_bar.is_open {
                self.start_find_scan(seq, cx);
            }
        }
    }

    /// Whether the match list no longer reflects the query or the document.
    pub(super) fn find_is_stale(&self) -> bool {
        if self.find_bar.matches_query != self.find_bar.query.text() {
            return true;
        }
        // Flipping `Aa` or whole-word changes what the ranges mean, even
        // though the query text and the document are the same.
        if self.find_bar.matches_options != self.find_bar.options() {
            return true;
        }
        self.state
            .active_document()
            .map(|doc| doc.version != self.find_bar.matches_version)
            .unwrap_or(true)
    }

    /// Whether a scan for the current query, toggles and document version is
    /// already on its way (or installed), so a repeated action should not
    /// restart it.
    pub(super) fn find_request_is_current(&self) -> bool {
        self.find_bar.search_query == self.find_bar.query.text()
            && self.find_bar.search_options == self.find_bar.options()
            && self
                .state
                .active_document()
                .map(|doc| doc.version == self.find_bar.search_version)
                .unwrap_or(false)
    }

    /// Makes match `index` the active one and, when `reveal` is set, scrolls
    /// it into view.
    ///
    /// The document caret and selection are left alone: the only active caret
    /// belongs to the find field, so the editor keeps its own cursor the way an
    /// unfocused split pane does. Typing a query must not move the view either,
    /// which is why automatic matches arrive with `reveal == false`.
    pub(super) fn find_activate(&mut self, index: usize, reveal: bool, cx: &mut Context<Self>) {
        let Some(range) = self.find_bar.matches.get(index).cloned() else { return; };
        self.find_bar.active = index;
        if reveal {
            let target = self.state.active_document().map(|doc| {
                let line = doc.line_of_byte(range.start);
                let line_start = doc.line_start_byte(line);
                let col = doc
                    .slice_to_string(line_start..range.start.min(doc.len()))
                    .chars()
                    .count();
                (line, col)
            });
            if let Some((line, col)) = target {
                self.scroll_visual_row_into_view(line, col);
            }
        }
        cx.notify();
    }

    /// Moves to the next (`delta > 0`) or previous match, wrapping around.
    pub(super) fn find_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.find_bar.query.is_empty() {
            return;
        }
        if self.find_is_stale() {
            // Wait for the background list; the reply activates first/last.
            self.find_bar.pending_step = Some(delta);
            if !self.find_request_is_current() {
                self.request_find_search(cx);
            }
            return;
        }
        let total = self.find_bar.matches.len();
        if total == 0 {
            cx.notify();
            return;
        }
        let current = self.find_bar.active.min(total - 1);
        let next = if delta >= 0 {
            (current + 1) % total
        } else {
            (current + total - 1) % total
        };
        self.find_activate(next, true, cx);
    }

    /// Flips case sensitivity of the find bar and re-runs the scan.
    pub(super) fn toggle_find_case(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find_bar.case_sensitive = !self.find_bar.case_sensitive;
        self.rescan_after_find_option(window, cx);
    }

    /// Flips whole-word matching of the find bar and re-runs the scan.
    pub(super) fn toggle_find_whole_word(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find_bar.whole_word = !self.find_bar.whole_word;
        self.rescan_after_find_option(window, cx);
    }

    /// Re-runs the find scan after a matching switch changed.
    ///
    /// The ranges and every pending action were computed under the old rules,
    /// so they are dropped first: a queued step must not land on a list that no
    /// longer matches what the user asked for, and a running Replace All is
    /// cancelled because it would rewrite text by the old rule.
    pub(super) fn rescan_after_find_option(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The ranges were found under the old rules: drop them now so the
        // counter never pairs new rules with an old count, and the buttons
        // stay disabled until the new pass lands.
        self.find_bar.matches.clear();
        self.find_bar.matches_query.clear();
        self.find_bar.matches_version = 0;
        self.find_bar.active = 0;
        self.find_bar.truncated = false;
        self.find_bar.pending_step = None;
        self.find_bar.pending_after = None;
        self.find_bar.pending_replace = false;
        self.find_bar.pending_mark_all = false;
        self.find_bar.cancel_replace_all();
        self.find_bar.query.focus(window);
        self.find_bar.query.blink.pause_and_show(BlinkTarget::FindQuery, cx);
        self.request_find_search(cx);
    }

    /// Replaces the active match with the replace field and moves to the next.
    pub(super) fn find_replace_current(&mut self, cx: &mut Context<Self>) {
        if self.find_bar.replace_all.is_some() {
            return;
        }
        let query = self.find_bar.query.text().to_string();
        if query.is_empty() {
            return;
        }
        if self.find_is_stale() {
            // A stale byte range must never be written to; the reply will
            // provide the ranges for the current document and then run this
            // replacement, so the click is not lost.
            self.find_bar.pending_replace = true;
            if !self.find_request_is_current() {
                self.request_find_search(cx);
            }
            return;
        }
        self.find_bar.pending_replace = false;
        let Some(range) = self
            .find_bar
            .matches
            .get(self.find_bar.active)
            .cloned()
        else {
            return;
        };
        let replacement = self.find_bar.replace.text().to_string();
        if replacement == query {
            // Replacing text with itself would only add an undo step.
            self.find_step(1, cx);
            return;
        }
        let inserted_len = replacement.len();
        let new_cursor = range.start + inserted_len;
        let replaced = range.clone();
        self.state.active_replace_range(range, replacement, new_cursor);
        // Typing schedules the save the same way; a synchronous write here
        // would block the UI for a whole large file on every replacement.
        self.schedule_auto_save(cx);
        self.refresh_view_after_cursor_move();
        // Only the neighbourhood of the edit can differ from the previous list,
        // so it is patched in place: the highlight stays on screen instead of
        // waiting for a full rescan of the document.
        if !self.find_patch_after_replace(replaced, inserted_len, new_cursor, cx) {
            self.find_bar.pending_after = Some(new_cursor);
            self.request_find_search(cx);
        }
    }

    /// Recomputes the match list after one replacement, without a full scan.
    ///
    /// Matches ending before the rescanned window stay as they are, matches
    /// starting after it shift by the length difference, and the window around
    /// the edit is scanned again for matches the replacement created or
    /// destroyed. Returns `false` when the numbers make a full scan the better
    /// option (a large query or a large inserted text).
    pub(super) fn find_patch_after_replace(
        &mut self,
        replaced: std::ops::Range<usize>,
        inserted_len: usize,
        new_cursor: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        // A query this long, or an insertion this large, would make the
        // window as expensive as the scan it is meant to save.
        const MARGIN_LIMIT: usize = 64 * 1024;
        const INSERTED_LIMIT: usize = 256 * 1024;
        if inserted_len > INSERTED_LIMIT {
            return false;
        }
        let query = self.find_bar.query.text().to_string();
        if query.is_empty() {
            return false;
        }
        // Context wide enough for a whole match on either side of the edit.
        let margin = query
            .chars()
            .count()
            .saturating_mul(12)
            .saturating_add(8);
        if margin > MARGIN_LIMIT {
            return false;
        }
        // The window and the current version, read together so the ranges and
        // the text they describe cannot come from different revisions.
        let Some((version, scan_start, owned_end, left_char, window)) =
            self.find_patch_window(&replaced, inserted_len, margin)
        else {
            return false;
        };
        let options = self.find_bar.options();
        let old = std::mem::take(&mut self.find_bar.matches);
        let mut patched = crate::views::find_bar::patch_matches(
            &old,
            &replaced,
            inserted_len,
            scan_start,
            owned_end,
            &window,
            &query,
            options,
            left_char,
            crate::views::find_bar::MAX_FIND_MATCHES,
        );
        if patched.len() > crate::views::find_bar::MAX_FIND_MATCHES {
            patched.truncate(crate::views::find_bar::MAX_FIND_MATCHES);
            self.find_bar.truncated = true;
        }
        self.find_bar.matches = patched;
        self.find_bar.matches_query = query;
        self.find_bar.matches_options = options;
        self.find_bar.matches_version = version;
        if self.find_bar.matches.is_empty() {
            // The replacement removed the last match: the highlight has to go
            // away even though there is nothing to activate.
            self.find_bar.active = 0;
            self.find_bar.truncated = false;
            cx.notify();
            return true;
        }
        let index = self
            .find_bar
            .matches
            .iter()
            .position(|m| m.start >= new_cursor)
            .unwrap_or(0);
        self.find_activate(index, true, cx);
        true
    }

    /// Slice around a replacement: one match length of context on each side,
    /// plus the same context again on the right for a match that starts just
    /// inside and ends outside. All bounds are snapped to character
    /// boundaries. Returns the document version the slice belongs to and the
    /// character just before it, which whole-word matching needs.
    pub(super) fn find_patch_window(
        &self,
        replaced: &std::ops::Range<usize>,
        inserted_len: usize,
        margin: usize,
    ) -> Option<(u64, usize, usize, Option<char>, String)> {
        let doc = self.state.active_document()?;
        let version = doc.version;
        let doc_len = doc.len();
        let mut scan_start = replaced.start.saturating_sub(margin);
        while scan_start > 0 && !doc.is_char_boundary(scan_start) {
            scan_start -= 1;
        }
        let mut owned_end = replaced
            .start
            .saturating_add(inserted_len)
            .saturating_add(margin)
            .min(doc_len);
        while owned_end < doc_len && !doc.is_char_boundary(owned_end) {
            owned_end += 1;
        }
        let mut scan_end = owned_end.saturating_add(margin).min(doc_len);
        while scan_end < doc_len && !doc.is_char_boundary(scan_end) {
            scan_end += 1;
        }
        let left_char = if scan_start == 0 {
            None
        } else {
            let mut from = scan_start.saturating_sub(4);
            while from < scan_start && !doc.is_char_boundary(from) {
                from += 1;
            }
            doc.slice_to_string(from..scan_start).chars().next_back()
        };
        Some((
            version,
            scan_start,
            owned_end,
            left_char,
            doc.slice_to_string(scan_start..scan_end),
        ))
    }

    /// Replaces every match in one edit, so undo treats it as a single step.
    ///
    /// The scan and the new text are built on a background thread: doing it in
    /// the click handler meant copying the whole document, scanning it without
    /// a limit and assembling a second copy while the UI thread waited, which
    /// on a large file froze the window for seconds.
    pub(super) fn find_replace_all(&mut self, cx: &mut Context<Self>) {
        let query = self.find_bar.query.text().to_string();
        // `0/0` means the search found nothing, so there is nothing to
        // replace; the document is not even scanned again.
        if query.is_empty()
            || self.find_bar.matches.is_empty()
            || self.find_bar.replace_all.is_some()
        {
            return;
        }
        let replacement = self.find_bar.replace.text().to_string();
        if replacement == query {
            // Replacing text with itself would only add an undo step. The
            // installed list still stands, so there is nothing to rescan.
            if self.find_is_stale() {
                self.request_find_search(cx);
            }
            return;
        }
        let Some(doc) = self.state.active_document() else {
            return;
        };
        let old_len = doc.len();
        if old_len == 0 {
            self.request_find_search(cx);
            return;
        }
        let version = doc.version;
        let path = doc.path.clone();
        let rope = doc.rope().clone();
        let options = self.find_bar.options();

        // An abandoned pass (the query changed while it ran) is stopped here
        // instead of being watched.
        self.find_bar.cancel_replace_all();
        let seq = self.find_bar.replace_all_seq;
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.find_bar.replace_cancel = std::sync::Arc::clone(&cancel);
        let progress = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        self.find_bar.replace_all = Some((0, old_len));

        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let background = cx.background_executor().clone();

        // The replace field shows the percentage at its right edge. Repainting
        // once per slice would flood the frame queue on a large file, so a
        // ticker samples the shared byte counter instead and stops as soon as
        // the pass is gone.
        let ticker_entity = entity.clone();
        let mut ticker_cx = cx.to_async();
        let ticker_executor = cx.background_executor().clone();
        let ticker_progress = std::sync::Arc::clone(&progress);
        cx.foreground_executor()
            .spawn(async move {
                loop {
                    ticker_executor
                        .timer(std::time::Duration::from_millis(100))
                        .await;
                    let alive = ticker_entity
                        .update(&mut ticker_cx, |this, cx| {
                            // A newer pass (or a cancelled one) owns the counter
                            // now; this ticker must not report stale numbers.
                            if this.find_bar.replace_all_seq != seq
                                || this.find_bar.replace_all.is_none()
                            {
                                return false;
                            }
                            let done = ticker_progress
                                .load(std::sync::atomic::Ordering::Relaxed);
                            this.find_bar.replace_all = Some((done, old_len));
                            cx.notify();
                            true
                        })
                        .unwrap_or(false);
                    if !alive {
                        break;
                    }
                }
            })
            .detach();

        cx.foreground_executor()
            .spawn(async move {
                let result = background
                    .spawn(async move {
                        let progress = progress;
                        crate::views::find_bar::replace_rope(
                            &rope,
                            &query,
                            &replacement,
                            options,
                            &|| cancel.load(std::sync::atomic::Ordering::Relaxed),
                            &|done| progress.store(done, std::sync::atomic::Ordering::Relaxed),
                        )
                    })
                    .await;
                let _ = entity.update(&mut async_cx, |this, cx| {
                    this.finish_find_replace_all(seq, version, path, old_len, result, cx);
                });
            })
            .detach();
        cx.notify();
    }

    /// Applies a finished Replace All pass, unless the document moved on.
    pub(super) fn finish_find_replace_all(
        &mut self,
        seq: u64,
        version: u64,
        path: std::path::PathBuf,
        old_len: usize,
        result: Option<crate::views::find_bar::ReplaceAllText>,
        cx: &mut Context<Self>,
    ) {
        if seq != self.find_bar.replace_all_seq || self.find_bar.replace_all.is_none() {
            return;
        }
        self.find_bar.replace_all = None;
        let Some(result) = result else {
            cx.notify();
            return;
        };
        // The replacement is built from a snapshot; applying it to a document
        // that was edited or switched meanwhile would destroy those edits.
        let same_document = self
            .state
            .active_document()
            .map(|doc| doc.path == path && doc.version == version && doc.len() == old_len)
            .unwrap_or(false);
        if !same_document {
            self.request_find_search(cx);
            return;
        }
        if result.replacements == 0 {
            // The document did not change, so the installed list is still
            // correct; only the progress label has to come back.
            cx.notify();
            return;
        }
        let new_cursor = result.first_end.unwrap_or(0).min(result.text.len());
        self.state
            .active_replace_range(0..old_len, result.text, new_cursor);
        // Same debounced save as typing: writing the file is not part of the
        // replace action itself.
        self.schedule_auto_save(cx);
        self.refresh_view_after_cursor_move();
        // Everything the query matched was just replaced, so the old list is
        // not only stale but meaningless: scanning the document again would
        // find nothing and only make the counter flash the previous numbers.
        // The list is dropped and marked stale instead — Enter, Replace or
        // reopening the bar rescans on demand (and if the replacement itself
        // recreates matches, that rescan brings them back).
        self.find_bar.matches.clear();
        self.find_bar.matches_query.clear();
        self.find_bar.matches_version = 0;
        self.find_bar.clear_marks();
        self.find_bar.active = 0;
        self.find_bar.truncated = false;
        cx.notify();
    }

    /// Marks every line that holds a match of the current query ("Mark all").
    ///
    /// Pressing it again with the same result clears the marks; changing the
    /// query, closing the bar or editing the document clears them as well,
    /// because a line number stops meaning the same line after an edit.
    pub(super) fn find_mark_all(&mut self, cx: &mut Context<Self>) {
        if self.find_bar.query.is_empty() {
            self.find_bar.clear_marks();
            cx.notify();
            return;
        }
        if let Some(doc) = self.state.active_document() {
            if self.find_bar.marks_valid_for(doc.id, doc.version) {
                self.find_bar.clear_marks();
                cx.notify();
                return;
            }
        }
        if self.find_is_stale() {
            // The ranges on screen belong to another revision or query; the
            // reply marks its own fresh list instead of these lines.
            self.find_bar.pending_mark_all = true;
            if !self.find_request_is_current() {
                self.request_find_search(cx);
            }
            return;
        }
        self.find_bar.pending_mark_all = false;
        let Some(doc) = self.state.active_document() else { return; };
        let mut lines: Vec<usize> = self
            .find_bar
            .matches
            .iter()
            .map(|range| doc.line_of_byte(range.start))
            .collect();
        lines.sort_unstable();
        lines.dedup();
        let doc_id = doc.id;
        let version = doc.version;
        self.find_bar.marked_lines = lines;
        self.find_bar.marked_doc = Some(doc_id);
        self.find_bar.marked_version = version;
        cx.notify();
    }

    /// Removes every line marked by "Mark all" from the document in one edit,
    /// so a single undo brings all of them back.
    pub(crate) fn delete_marked_lines(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.state.active_document() else { return; };
        if !self.find_bar.marks_valid_for(doc.id, doc.version) {
            return;
        }
        let lines = self.find_bar.marked_lines.clone();
        let (Some(first), Some(last)) = (lines.first().copied(), lines.last().copied()) else {
            return;
        };
        let total_lines = doc.line_index.lines();
        let span_start = doc.line_start_byte(first);
        let ends_document = last + 1 >= total_lines;
        let span_end = if ends_document {
            doc.len()
        } else {
            doc.line_start_byte(last + 1)
        };
        let (replacement, any_kept) = crate::views::find_bar::delete_marked_lines_replacement(
            &doc.slice_to_string(span_start..span_end),
            first,
            &lines,
            ends_document,
        );
        // Nothing of the tail survived: the newline that ended the previous
        // line goes too, or the document would keep an empty last line.
        let mut start = span_start;
        if !any_kept && ends_document && first > 0 {
            let crlf = start >= 2 && doc.rope().byte(start - 2) == b'\r';
            start -= if crlf { 2 } else { 1 };
        }
        if start >= span_end {
            self.find_bar.clear_marks();
            cx.notify();
            return;
        }
        self.state
            .active_replace_range(start..span_end, replacement, start);
        self.schedule_auto_save(cx);
        self.refresh_view_after_cursor_move();
        self.find_bar.clear_marks();
        if self.find_bar.is_open {
            // The highlights and the counter describe lines that are gone.
            self.request_find_search(cx);
        }
        cx.notify();
    }

    /// Key handling for the find bar's two fields.
    pub(super) fn handle_find_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let ctrl = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
        let shift = event.keystroke.modifiers.shift;

        if key == "escape" {
            self.close_find_bar(window, cx);
            return;
        }
        if ctrl && (key.eq_ignore_ascii_case("f") || key == "а" || key == "А") {
            self.find_bar.replace.blink.disable(cx);
            self.find_bar.query.focus(window);
            self.find_bar.query.select_all();
            self.find_bar.query.blink.pause_and_show(BlinkTarget::FindQuery, cx);
            cx.notify();
            return;
        }
        if key == "enter" {
            self.find_step(if shift { -1 } else { 1 }, cx);
            return;
        }

        let query_focused = self.find_bar.query.is_focused(window);
        if !query_focused && !self.find_bar.replace.is_focused(window) {
            return;
        }
        let changed = if query_focused {
            self.find_bar.query.handle_key_down(event, cx)
        } else {
            self.find_bar.replace.handle_key_down(event, cx)
        };
        if query_focused {
            if changed {
                // The field already shows the new text; the scan runs in the
                // background so holding a key never stalls on a large file.
                self.find_bar.pending_step = None;
                self.find_bar.pending_after = None;
                // The replacement that was waiting for ranges was meant for
                // the old query; applying it to a new one would be a surprise.
                self.find_bar.pending_replace = false;
                self.find_bar.pending_mark_all = false;
                // Same for a Replace All pass already rewriting the document.
                self.find_bar.cancel_replace_all();
                self.request_find_search(cx);
            }
            self.find_bar.query.blink.pause_and_show(BlinkTarget::FindQuery, cx);
        } else {
            self.find_bar.replace.blink.pause_and_show(BlinkTarget::FindReplace, cx);
        }
    }
}
