use super::*;

impl Default for ViewCache {
    fn default() -> Self {
        Self {
            window: EditorWindow::default(),
            above_lines: 0,
            below_lines: 0,
            above_h: 0.0,
            below_h: 0.0,
            est_rows_per_line: 1.0,
            est_row_h: crate::settings::EditorTypography::reference().row_height as f64,
            visual_cache_dirty: true,
            typography_cache: crate::settings::EditorTypography::reference(),
            raw_table: false,
            raw_tables: std::collections::HashSet::new(),
            drag_revealed_tables: std::collections::HashSet::new(),
            drag_revealed_images: std::collections::HashSet::new(),
            revealed_selection: None,
            cursor_line: 0,
            line_paints: std::collections::HashMap::new(),
            table_cell_cols: std::collections::HashMap::new(),
        }
    }
}

impl TableCellSelection {
    pub fn new(table_first_line: usize, row: usize, col: usize) -> Self {
        Self {
            table_first_line,
            anchor_row: row,
            anchor_col: col,
            head_row: row,
            head_col: col,
        }
    }

    pub fn rect(&self) -> crate::table::CellRect {
        crate::table::CellRect::new(self.anchor_row, self.anchor_col, self.head_row, self.head_col)
    }

    pub fn is_multi_cell(&self) -> bool {
        self.anchor_row != self.head_row || self.anchor_col != self.head_col
    }

    pub fn contains(&self, row: usize, col: usize) -> bool {
        self.rect().contains_cell(row, col)
    }
}
