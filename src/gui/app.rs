//! Core application state and business logic (UI-agnostic).

use gridline_core::{CellRef, Document};
use gridline_engine::engine::Cell;

struct InternalClipboard {
    text: String,
    source_col: usize,
    source_row: usize,
    cells: Vec<(usize, usize, Cell)>,
}

/// Core application state - contains spreadsheet data and business logic.
/// This is independent of the UI framework and can be tested in isolation.
pub struct GuiApp {
    pub doc: Document,
    pub selected: CellRef,
    pub selection_anchor: CellRef,
    pub selection_end: CellRef,
    pub edit_buffer: String,
    pub edit_dirty: bool,
    pub status: String,
    internal_clipboard: Option<InternalClipboard>,
}

impl GuiApp {
    pub fn new(doc: Document) -> Self {
        let selected = CellRef::new(0, 0);
        let mut app = Self {
            doc,
            selected: selected.clone(),
            selection_anchor: selected.clone(),
            selection_end: selected.clone(),
            edit_buffer: String::new(),
            edit_dirty: false,
            status: String::new(),
            internal_clipboard: None,
        };
        app.sync_edit_buffer();
        app
    }

    /// Get the display/input string for a cell.
    pub fn cell_input_string(&self, cell: &CellRef) -> String {
        self.doc
            .grid
            .get(cell)
            .map(|c| c.to_input_string())
            .unwrap_or_default()
    }

    /// Get the evaluated display value for a cell.
    pub fn cell_display(&mut self, cell: &CellRef) -> String {
        self.doc.get_cell_display(cell)
    }

    /// Sync edit buffer from currently selected cell.
    pub fn sync_edit_buffer(&mut self) {
        if let Some(cell) = self.doc.grid.get(&self.selected) {
            self.edit_buffer = cell.to_input_string();
        } else {
            self.edit_buffer.clear();
        }
        self.edit_dirty = false;
    }

    /// Calculate selection bounds from anchor and end.
    pub fn selection_bounds(&self) -> (usize, usize, usize, usize) {
        let c1 = self.selection_anchor.col.min(self.selection_end.col);
        let r1 = self.selection_anchor.row.min(self.selection_end.row);
        let c2 = self.selection_anchor.col.max(self.selection_end.col);
        let r2 = self.selection_anchor.row.max(self.selection_end.row);
        (c1, r1, c2, r2)
    }

    /// Human-readable label for current selection (e.g., "A1" or "A1:B5").
    pub fn selection_label(&self) -> String {
        let (c1, r1, c2, r2) = self.selection_bounds();
        if r1 == r2 && c1 == c2 {
            format!("{}", CellRef::new(c1, r1))
        } else {
            format!("{}:{}", CellRef::new(c1, r1), CellRef::new(c2, r2))
        }
    }

    /// Check if a cell is within the current selection.
    pub fn in_selection(&self, cell: &CellRef) -> bool {
        let (c1, r1, c2, r2) = self.selection_bounds();
        cell.row >= r1 && cell.row <= r2 && cell.col >= c1 && cell.col <= c2
    }

    /// Update selected cell and optionally extend selection range.
    pub fn set_selected(&mut self, cell: CellRef, extend_selection: bool) {
        self.selected = cell;
        if extend_selection {
            self.selection_end = self.selected.clone();
        } else {
            self.selection_anchor = self.selected.clone();
            self.selection_end = self.selected.clone();
        }
        self.sync_edit_buffer();
    }

    /// Move selection by relative offset (dx columns, dy rows).
    pub fn move_selection(&mut self, dx: isize, dy: isize, extend_selection: bool) {
        let r = self.selected.row as isize + dy;
        let c = self.selected.col as isize + dx;
        self.set_selected(
            CellRef::new(c.max(0) as usize, r.max(0) as usize),
            extend_selection,
        );
    }

    /// Set cell value from user input string.
    pub fn set_cell_from_input(&mut self, input: &str) -> Result<(), String> {
        match self.doc.set_cell_from_input(self.selected.clone(), input) {
            Ok(()) => {
                self.status = format!("Updated {}", self.selected);
                self.edit_dirty = false;
                Ok(())
            }
            Err(e) => {
                self.status = format!("Error: {}", e);
                Err(e.to_string())
            }
        }
    }

    /// Clear all cells in current selection.
    pub fn clear_selection(&mut self) {
        let (c1, r1, c2, r2) = self.selection_bounds();
        for r in r1..=r2 {
            for c in c1..=c2 {
                self.doc.clear_cell(&CellRef::new(c, r));
            }
        }
        self.sync_edit_buffer();
        self.status = format!("Cleared {}", self.selection_label());
    }

    /// Parse clipboard text into a 2D grid (handles tab/newline delimiters).
    pub fn parse_clipboard_grid(s: &str) -> Vec<Vec<String>> {
        let s = s.replace("\r\n", "\n").replace('\r', "\n");
        let mut lines: Vec<&str> = s.split('\n').collect();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        if lines.is_empty() {
            return Vec::new();
        }
        lines
            .iter()
            .map(|line| {
                if line.contains('\t') {
                    line.split('\t').map(|c| c.to_string()).collect()
                } else {
                    vec![line.to_string()]
                }
            })
            .collect()
    }

    /// Paste clipboard data into selection.
    pub fn paste_from_clipboard(&mut self, s: String) -> Result<usize, String> {
        let grid = Self::parse_clipboard_grid(&s);
        if grid.is_empty() {
            self.status = "Paste failed: empty clipboard".to_string();
            return Err("empty clipboard".to_string());
        }

        let (c1, r1, c2, r2) = self.selection_bounds();
        let sel_rows = r2 - r1 + 1;
        let sel_cols = c2 - c1 + 1;

        if let Some(clip) = self.internal_clipboard.as_ref()
            && clip.text == s
        {
            match self
                .doc
                .paste_cells(c1, r1, clip.source_col, clip.source_row, &clip.cells)
            {
                Ok(pasted) => {
                    self.sync_edit_buffer();
                    self.status =
                        format!("Pasted {} cell(s) into {}", pasted, self.selection_label());
                    Ok(pasted)
                }
                Err(e) => {
                    self.status = format!("Paste failed: {}", e);
                    Err(e.to_string())
                }
            }
        } else {
            let single_value = grid.len() == 1 && grid[0].len() == 1;
            let cells = if single_value && (sel_rows > 1 || sel_cols > 1) {
                let value = Cell::from_input(&grid[0][0]);
                let mut repeated = Vec::with_capacity(sel_rows * sel_cols);
                for dr in 0..sel_rows {
                    for dc in 0..sel_cols {
                        repeated.push((dc, dr, value.clone()));
                    }
                }
                repeated
            } else {
                let mut parsed = Vec::new();
                for (dr, row) in grid.iter().enumerate() {
                    for (dc, value) in row.iter().enumerate() {
                        parsed.push((dc, dr, Cell::from_input(value)));
                    }
                }
                parsed
            };

            match self.doc.paste_cells(c1, r1, c1, r1, &cells) {
                Ok(pasted) => {
                    self.sync_edit_buffer();
                    self.status =
                        format!("Pasted {} cell(s) into {}", pasted, self.selection_label());
                    Ok(pasted)
                }
                Err(e) => {
                    self.status = format!("Paste failed: {}", e);
                    Err(e.to_string())
                }
            }
        }
    }

    /// Copy current selection to string format (tab/newline delimited).
    pub fn copy_selection_to_string(&self) -> String {
        let (c1, r1, c2, r2) = self.selection_bounds();
        let mut out = String::new();
        for r in r1..=r2 {
            if r != r1 {
                out.push('\n');
            }
            for c in c1..=c2 {
                if c != c1 {
                    out.push('\t');
                }
                out.push_str(&self.cell_input_string(&CellRef::new(c, r)));
            }
        }
        out
    }

    /// Copy selection and keep a structured snapshot for internal formula-aware paste.
    pub fn copy_selection_to_string_and_store(&mut self) -> String {
        let text = self.copy_selection_to_string();
        let (c1, r1, c2, r2) = self.selection_bounds();
        let mut cells = Vec::new();
        for row in r1..=r2 {
            for col in c1..=c2 {
                let cell_ref = CellRef::new(col, row);
                let cell = self
                    .doc
                    .grid
                    .get(&cell_ref)
                    .map(|c| c.clone())
                    .unwrap_or_else(Cell::new_empty);
                cells.push((col - c1, row - r1, cell));
            }
        }
        self.internal_clipboard = Some(InternalClipboard {
            text: text.clone(),
            source_col: c1,
            source_row: r1,
            cells,
        });
        text
    }

    /// Undo last action.
    pub fn undo(&mut self) -> Result<(), String> {
        match self.doc.undo() {
            Ok(()) => {
                self.status = "Undo".to_string();
                self.sync_edit_buffer();
                Ok(())
            }
            Err(e) => {
                self.status = format!("Undo failed: {}", e);
                Err(e.to_string())
            }
        }
    }

    /// Redo last undone action.
    pub fn redo(&mut self) -> Result<(), String> {
        match self.doc.redo() {
            Ok(()) => {
                self.status = "Redo".to_string();
                self.sync_edit_buffer();
                Ok(())
            }
            Err(e) => {
                self.status = format!("Redo failed: {}", e);
                Err(e.to_string())
            }
        }
    }

    /// Delete row at index.
    pub fn delete_row(&mut self, at_row: usize) {
        self.doc.delete_row(at_row);
        self.status = format!("Deleted row {}", at_row + 1);
        self.sync_edit_buffer();
    }

    /// Delete column at index.
    pub fn delete_column(&mut self, at_col: usize) {
        self.doc.delete_column(at_col);
        self.status = format!("Deleted column {}", CellRef::col_to_letters(at_col));
        self.sync_edit_buffer();
    }

    /// Insert row before index.
    pub fn insert_row(&mut self, at_row: usize) {
        self.doc.insert_row(at_row);
        self.status = format!("Inserted row before {}", at_row + 1);
        self.sync_edit_buffer();
    }

    /// Insert column before index.
    pub fn insert_column(&mut self, at_col: usize) {
        self.doc.insert_column(at_col);
        self.status = format!("Inserted column before {}", CellRef::col_to_letters(at_col));
        self.sync_edit_buffer();
    }

    /// Save document to file.
    pub fn save(&mut self) -> Result<String, String> {
        match self.doc.save_file() {
            Ok(p) => {
                let path_str = p.display().to_string();
                self.status = format!("Saved {}", path_str);
                Ok(path_str)
            }
            Err(e) => {
                let err_msg = e.to_string();
                self.status = format!("Save failed: {}", err_msg);
                Err(err_msg)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridline_engine::engine::CellType;

    #[test]
    fn test_selection_bounds_and_label_order() {
        let doc = Document::new();
        let mut app = GuiApp::new(doc);
        app.selection_anchor = CellRef::new(2, 1);
        app.selection_end = CellRef::new(4, 3);

        assert_eq!(app.selection_bounds(), (2, 1, 4, 3));
        assert_eq!(app.selection_label(), "C2:E4");
    }

    #[test]
    fn test_internal_copy_paste_shifts_formula_references() {
        let mut doc = Document::new();
        doc.set_cell_from_input(CellRef::new(0, 0), "=B1").unwrap(); // A1

        let mut app = GuiApp::new(doc);
        let copied = app.copy_selection_to_string_and_store();
        app.set_selected(CellRef::new(0, 1), false); // A2
        app.paste_from_clipboard(copied).unwrap();

        let pasted = app.doc.grid.get(&CellRef::new(0, 1)).unwrap();
        match &pasted.contents {
            CellType::Script(s) => assert_eq!(s, "B2"),
            _ => panic!("Expected formula cell"),
        }
    }

    #[test]
    fn test_external_paste_rejects_circular_formula() {
        let mut doc = Document::new();
        doc.set_cell_from_input(CellRef::new(0, 0), "=B1").unwrap(); // A1 depends on B1

        let mut app = GuiApp::new(doc);
        app.set_selected(CellRef::new(1, 0), false); // B1
        let result = app.paste_from_clipboard("=A1".to_string());

        assert!(result.is_err());
        assert!(app.doc.grid.get(&CellRef::new(1, 0)).is_none());
        assert!(app.status.starts_with("Paste failed:"));
    }

    #[test]
    fn test_single_value_paste_fills_selection() {
        let doc = Document::new();
        let mut app = GuiApp::new(doc);
        app.selection_anchor = CellRef::new(0, 0);
        app.selection_end = CellRef::new(1, 1); // 2x2 selection
        app.selected = CellRef::new(0, 0);

        let pasted = app.paste_from_clipboard("7".to_string()).unwrap();

        assert_eq!(pasted, 4);
        assert_eq!(app.cell_input_string(&CellRef::new(0, 0)), "7");
        assert_eq!(app.cell_input_string(&CellRef::new(1, 0)), "7");
        assert_eq!(app.cell_input_string(&CellRef::new(0, 1)), "7");
        assert_eq!(app.cell_input_string(&CellRef::new(1, 1)), "7");
    }
}
