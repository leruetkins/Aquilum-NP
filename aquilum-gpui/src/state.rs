use aquilum_core::app_core::{Core, CoreEvent, EventSink};
use aquilum_core::files::models::{FileItem, FileItemType};
use aquilum_core::files::workspace::read_directory_impl;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

pub struct OpenedNote {
    pub path: PathBuf,
    pub title: String,
    pub content: String,
    #[allow(dead_code)]
    pub version: u64,
    pub cursor_offset: usize,
    pub selection_anchor: Option<usize>,
    pub is_dirty: bool,
    pub undo_stack: Vec<(String, usize)>,
    pub redo_stack: Vec<(String, usize)>,
}

impl OpenedNote {
    pub fn new(path: PathBuf, title: String, content: String, version: u64) -> Self {
        Self {
            path,
            title,
            content,
            version,
            cursor_offset: 0,
            selection_anchor: None,
            is_dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn snapshot_undo(&mut self) {
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push((self.content.clone(), self.cursor_offset));
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some((prev_content, prev_cursor)) = self.undo_stack.pop() {
            self.redo_stack.push((self.content.clone(), self.cursor_offset));
            self.content = prev_content;
            self.cursor_offset = prev_cursor.min(self.content.len());
            self.selection_anchor = None;
            self.is_dirty = true;
        }
    }

    pub fn redo(&mut self) {
        if let Some((next_content, next_cursor)) = self.redo_stack.pop() {
            self.undo_stack.push((self.content.clone(), self.cursor_offset));
            self.content = next_content;
            self.cursor_offset = next_cursor.min(self.content.len());
            self.selection_anchor = None;
            self.is_dirty = true;
        }
    }

    pub fn selection_range(&self) -> Option<Range<usize>> {
        if let Some(anchor) = self.selection_anchor {
            if anchor != self.cursor_offset {
                let start = anchor.min(self.cursor_offset).min(self.content.len());
                let end = anchor.max(self.cursor_offset).min(self.content.len());
                return Some(start..end);
            }
        }
        None
    }

    pub fn selected_text(&self) -> Option<String> {
        let range = self.selection_range()?;
        Some(self.content[range].to_string())
    }

    pub fn delete_selection(&mut self) -> bool {
        if let Some(range) = self.selection_range() {
            self.snapshot_undo();
            self.content.replace_range(range.clone(), "");
            self.cursor_offset = range.start;
            self.selection_anchor = None;
            self.is_dirty = true;
            true
        } else {
            false
        }
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor_offset = self.content.len();
    }

    /// Возвращает (line_index, col_index) 0-indexed
    pub fn line_col(&self) -> (usize, usize) {
        let offset = self.cursor_offset.min(self.content.len());
        let prefix = &self.content[..offset];
        let line_idx = prefix.matches('\n').count();
        let last_newline = prefix.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let col_idx = prefix[last_newline..].chars().count();
        (line_idx, col_idx)
    }

    pub fn offset_for_line_col(&self, target_line: usize, target_col: usize) -> usize {
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                let char_count = line.chars().count();
                let actual_col = target_col.min(char_count);
                let byte_col: usize = line.chars().take(actual_col).map(|c| c.len_utf8()).sum();
                return offset + byte_col;
            }
            offset += line.len() + 1;
        }
        self.content.len()
    }

    pub fn insert_text(&mut self, text: &str) {
        self.snapshot_undo();
        let _ = self.delete_selection();
        let offset = self.cursor_offset.min(self.content.len());
        self.content.insert_str(offset, text);
        self.cursor_offset = offset + text.len();
        self.selection_anchor = None;
        self.is_dirty = true;
    }

    pub fn insert_newline_auto_indent(&mut self) {
        self.snapshot_undo();
        let _ = self.delete_selection();

        let offset = self.cursor_offset.min(self.content.len());
        let prefix = &self.content[..offset];
        let last_line = prefix.split('\n').last().unwrap_or("");
        let indent_len = last_line.chars().take_while(|c| c.is_whitespace()).count();
        let indent: String = last_line.chars().take(indent_len).collect();

        let insert_str = format!("\n{}", indent);
        self.content.insert_str(offset, &insert_str);
        self.cursor_offset = offset + insert_str.len();
        self.selection_anchor = None;
        self.is_dirty = true;
    }

    pub fn delete_backwards(&mut self) {
        if self.delete_selection() {
            return;
        }
        if self.cursor_offset == 0 || self.content.is_empty() {
            return;
        }
        self.snapshot_undo();
        let offset = self.cursor_offset.min(self.content.len());
        let mut prev = offset - 1;
        while prev > 0 && !self.content.is_char_boundary(prev) {
            prev -= 1;
        }
        self.content.remove(prev);
        self.cursor_offset = prev;
        self.selection_anchor = None;
        self.is_dirty = true;
    }

    pub fn delete_forwards(&mut self) {
        if self.delete_selection() {
            return;
        }
        let offset = self.cursor_offset.min(self.content.len());
        if offset >= self.content.len() {
            return;
        }
        self.snapshot_undo();
        self.content.remove(offset);
        self.selection_anchor = None;
        self.is_dirty = true;
    }

    pub fn move_cursor_left(&mut self, shift: bool) {
        if !shift && self.selection_anchor.is_some() {
            if let Some(range) = self.selection_range() {
                self.cursor_offset = range.start;
                self.selection_anchor = None;
                return;
            }
        }
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        if self.cursor_offset == 0 {
            return;
        }
        let mut prev = self.cursor_offset - 1;
        while prev > 0 && !self.content.is_char_boundary(prev) {
            prev -= 1;
        }
        self.cursor_offset = prev;
        if !shift {
            self.selection_anchor = None;
        }
    }

    pub fn move_cursor_right(&mut self, shift: bool) {
        if !shift && self.selection_anchor.is_some() {
            if let Some(range) = self.selection_range() {
                self.cursor_offset = range.end;
                self.selection_anchor = None;
                return;
            }
        }
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        if self.cursor_offset >= self.content.len() {
            return;
        }
        let mut next = self.cursor_offset + 1;
        while next <= self.content.len() && !self.content.is_char_boundary(next) {
            next += 1;
        }
        if next <= self.content.len() {
            self.cursor_offset = next;
        }
        if !shift {
            self.selection_anchor = None;
        }
    }

    pub fn move_cursor_up(&mut self, shift: bool) {
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        let (line_idx, col_idx) = self.line_col();
        if line_idx == 0 {
            self.cursor_offset = 0;
            if !shift {
                self.selection_anchor = None;
            }
            return;
        }

        let lines: Vec<&str> = self.content.split('\n').collect();
        let target_line = line_idx - 1;
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                let char_count = line.chars().count();
                let actual_col = col_idx.min(char_count);
                let byte_col: usize = line.chars().take(actual_col).map(|c| c.len_utf8()).sum();
                self.cursor_offset = offset + byte_col;
                if !shift {
                    self.selection_anchor = None;
                }
                return;
            }
            offset += line.len() + 1;
        }
    }

    pub fn move_cursor_down(&mut self, shift: bool) {
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        let (line_idx, col_idx) = self.line_col();
        let lines: Vec<&str> = self.content.split('\n').collect();
        if line_idx + 1 >= lines.len() {
            self.cursor_offset = self.content.len();
            if !shift {
                self.selection_anchor = None;
            }
            return;
        }

        let target_line = line_idx + 1;
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                let char_count = line.chars().count();
                let actual_col = col_idx.min(char_count);
                let byte_col: usize = line.chars().take(actual_col).map(|c| c.len_utf8()).sum();
                self.cursor_offset = offset + byte_col;
                if !shift {
                    self.selection_anchor = None;
                }
                return;
            }
            offset += line.len() + 1;
        }
    }

    pub fn set_cursor_to_line_start(&mut self, target_line: usize) {
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                self.cursor_offset = offset;
                self.selection_anchor = None;
                return;
            }
            offset += line.len() + 1;
        }
    }

    pub fn set_cursor_to_line_end(&mut self, target_line: usize) {
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                self.cursor_offset = offset + line.len();
                self.selection_anchor = None;
                return;
            }
            offset += line.len() + 1;
        }
    }

    #[allow(dead_code)]
    pub fn set_cursor_to_line_col(&mut self, target_line: usize, target_col: usize) {
        self.cursor_offset = self.offset_for_line_col(target_line, target_col);
        self.selection_anchor = None;
    }

    pub fn home(&mut self, shift: bool) {
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        let (line_idx, _) = self.line_col();
        self.set_cursor_to_line_start(line_idx);
        if !shift {
            self.selection_anchor = None;
        }
    }

    pub fn end(&mut self, shift: bool) {
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        let (line_idx, _) = self.line_col();
        self.set_cursor_to_line_end(line_idx);
        if !shift {
            self.selection_anchor = None;
        }
    }
}

pub struct AppState {
    pub core: Arc<Core>,
    pub workspace_path: Option<PathBuf>,
    pub workspace_name: String,
    pub files: Vec<FileItem>,
    pub active_note: Option<OpenedNote>,
    pub status_text: String,
}

struct CoreEventSink;

impl EventSink for CoreEventSink {
    fn emit(&self, event: CoreEvent) {
        println!("[core event] {}", event.name());
    }
}

const DEFAULT_WELCOME_NOTE: &str = "# Welcome to Aquilum (GPUI Live Preview)\n\nYour fast, distraction-free markdown knowledge base with **true Obsidian Live Preview**!\n\n## Inline Live Preview Demo\n- Click any line or block to edit raw markdown inline\n- Move cursor away (or click another line) to see rendered styling\n- Regular text and code blocks are fully selectable across the entire page with `Ctrl+A`\n\n### Interactive Features\n- [x] Zero Electron / Chromium overhead (~40 MB RAM, 120 FPS)\n- [x] Full document continuous text selection\n- [x] Dedicated Obsidian-style code blocks with syntax styling\n- [ ] Auto-saving and instant search\n\n```rust\nfn main() {\n    println!(\"Hello from Aquilum GPUI Live Preview!\");\n}\n```\n\n> Regular text and code blocks are cleanly organized with seamless Live Preview transitions.\n";

impl AppState {
    pub fn new() -> Self {
        let app_data_dir = dirs::data_dir()
            .map(|d| d.join("com.dmitriy.aquilum-app"))
            .unwrap_or_else(|| PathBuf::from("./data"));

        let _ = std::fs::create_dir_all(&app_data_dir);
        let sink: Arc<dyn EventSink> = Arc::new(CoreEventSink);
        let core = Core::open(&app_data_dir, sink);

        let mut state = Self {
            core,
            workspace_path: None,
            workspace_name: "No Workspace".to_owned(),
            files: Vec::new(),
            active_note: None,
            status_text: "GPUI Ready • 0 WebView Processes".to_owned(),
        };

        if let Ok(workspaces) = state.core.ui_state.list_workspaces() {
            if let Some(first) = workspaces.into_iter().next() {
                let path = PathBuf::from(&first.path);
                if path.exists() {
                    state.open_workspace(path);
                }
            }
        }

        if state.active_note.is_none() {
            state.active_note = Some(OpenedNote::new(
                PathBuf::from("Welcome.md"),
                "Welcome to Aquilum".to_owned(),
                DEFAULT_WELCOME_NOTE.to_owned(),
                0,
            ));
        }

        state
    }

    pub fn open_workspace(&mut self, path: PathBuf) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Workspace".to_owned());

        self.workspace_name = name;
        self.workspace_path = Some(path.clone());
        self.reload_files();
        self.status_text = format!("Workspace: {}", self.workspace_name);

        if self.active_note.is_none() {
            if let Some(first_md) = self.files.iter().find(|f| f.item_type == FileItemType::File) {
                let note_path = PathBuf::from(&first_md.id);
                self.open_note(note_path);
            }
        }
    }

    pub fn reload_files(&mut self) {
        if let Some(ref ws) = self.workspace_path {
            match read_directory_impl(ws) {
                Ok(items) => self.files = items,
                Err(err) => eprintln!("Failed to read directory {}: {:?}", ws.display(), err),
            }
        }
    }

    pub fn open_note(&mut self, path: PathBuf) {
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned());

        match self.core.documents.open(&self.core, &path, 0) {
            Ok(opened) => {
                self.active_note = Some(OpenedNote::new(
                    path.clone(),
                    title,
                    opened.text,
                    opened.version,
                ));
                self.status_text = format!("Opened: {}", path.file_name().unwrap_or_default().to_string_lossy());
            }
            Err(_) => {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    self.active_note = Some(OpenedNote::new(
                        path.clone(),
                        title,
                        content,
                        0,
                    ));
                    self.status_text = format!("Read: {}", path.file_name().unwrap_or_default().to_string_lossy());
                }
            }
        }
    }

    pub fn save_active_note(&mut self) {
        if let Some(ref mut note) = self.active_note {
            if let Err(err) = std::fs::write(&note.path, &note.content) {
                self.status_text = format!("Save error: {}", err);
            } else {
                note.is_dirty = false;
                self.status_text = format!("Saved: {}", note.title);
            }
        }
    }
}
