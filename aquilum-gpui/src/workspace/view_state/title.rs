use super::*;

impl TitleInput {
    pub fn new(text: String) -> Self {
        let cursor_pos = text.chars().count();
        Self {
            text,
            cursor_pos,
            selection_anchor: None,
            is_focused: true,
            is_mouse_selecting: false,
        }
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        if anchor != self.cursor_pos {
            let start = anchor.min(self.cursor_pos);
            let end = anchor.max(self.cursor_pos);
            Some((start, end))
        } else {
            None
        }
    }

    pub fn selected_text(&self) -> Option<String> {
        let (s_start, s_end) = self.selection_range()?;
        Some(self.text.chars().skip(s_start).take(s_end - s_start).collect())
    }

    pub fn delete_selection(&mut self) -> bool {
        if let Some((s_start, s_end)) = self.selection_range() {
            let mut chars: Vec<char> = self.text.chars().collect();
            if s_start < chars.len() {
                chars.drain(s_start..s_end.min(chars.len()));
                self.text = chars.into_iter().collect();
            }
            self.cursor_pos = s_start;
            self.selection_anchor = None;
            true
        } else {
            false
        }
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor_pos = self.text.chars().count();
    }

    pub fn insert_text(&mut self, text: &str) {
        self.delete_selection();
        let mut chars: Vec<char> = self.text.chars().collect();
        let pos = self.cursor_pos.min(chars.len());
        for (i, c) in text.chars().enumerate() {
            chars.insert(pos + i, c);
        }
        self.cursor_pos = pos + text.chars().count();
        self.text = chars.into_iter().collect();
    }

    pub fn move_left(&mut self, select: bool) {
        if select {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor_pos);
            }
            if self.cursor_pos > 0 {
                self.cursor_pos -= 1;
            }
        } else {
            if let Some((s_start, _)) = self.selection_range() {
                self.cursor_pos = s_start;
                self.selection_anchor = None;
            } else if self.cursor_pos > 0 {
                self.cursor_pos -= 1;
            }
        }
    }

    pub fn move_right(&mut self, select: bool) {
        let total_chars = self.text.chars().count();
        if select {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor_pos);
            }
            if self.cursor_pos < total_chars {
                self.cursor_pos += 1;
            }
        } else {
            if let Some((_, s_end)) = self.selection_range() {
                self.cursor_pos = s_end;
                self.selection_anchor = None;
            } else if self.cursor_pos < total_chars {
                self.cursor_pos += 1;
            }
        }
    }

    pub fn move_home(&mut self, select: bool) {
        if select {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor_pos);
            }
        } else {
            self.selection_anchor = None;
        }
        self.cursor_pos = 0;
    }

    pub fn move_end(&mut self, select: bool) {
        let total_chars = self.text.chars().count();
        if select {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor_pos);
            }
        } else {
            self.selection_anchor = None;
        }
        self.cursor_pos = total_chars;
    }

    pub fn backspace(&mut self) {
        if !self.delete_selection() {
            let mut chars: Vec<char> = self.text.chars().collect();
            let pos = self.cursor_pos.min(chars.len());
            if pos > 0 {
                chars.remove(pos - 1);
                self.cursor_pos = pos - 1;
                self.text = chars.into_iter().collect();
            }
        }
    }

    pub fn delete(&mut self) {
        if !self.delete_selection() {
            let mut chars: Vec<char> = self.text.chars().collect();
            let pos = self.cursor_pos.min(chars.len());
            if pos < chars.len() {
                chars.remove(pos);
                self.text = chars.into_iter().collect();
            }
        }
    }
}
