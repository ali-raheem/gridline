//! UI-specific state (viewport, focus, layout).

use gridline_core::CellRef;

/// Viewport and UI layout state.
pub struct GuiState {
    /// Top-left cell currently visible in viewport.
    pub viewport_row: usize,
    pub viewport_col: usize,

    /// Number of rows and columns visible in viewport.
    pub viewport_rows: usize,
    pub viewport_cols: usize,

    /// True if currently editing a cell.
    pub editing: bool,

    /// True if we should request focus on the formula bar this frame.
    pub request_focus_formula: bool,
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            viewport_row: 0,
            viewport_col: 0,
            viewport_rows: 30,
            viewport_cols: 12,
            editing: false,
            request_focus_formula: false,
        }
    }
}

impl GuiState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ensure selected cell is visible in viewport, scrolling if necessary.
    pub fn ensure_selected_visible(&mut self, selected: &CellRef) {
        let row = selected.row;
        let col = selected.col;

        // Scroll up if selected is above viewport
        if row < self.viewport_row {
            self.viewport_row = row;
        }
        // Scroll down if selected is below viewport
        else if row >= self.viewport_row + self.viewport_rows {
            self.viewport_row = row.saturating_sub(self.viewport_rows.saturating_sub(1));
        }

        // Scroll left if selected is before viewport
        if col < self.viewport_col {
            self.viewport_col = col;
        }
        // Scroll right if selected is after viewport
        else if col >= self.viewport_col + self.viewport_cols {
            self.viewport_col = col.saturating_sub(self.viewport_cols.saturating_sub(1));
        }
    }

    /// Calculate selection bounds from anchor and end points.
    pub fn selection_bounds(anchor: &CellRef, end: &CellRef) -> (usize, usize, usize, usize) {
        let c1 = anchor.col.min(end.col);
        let r1 = anchor.row.min(end.row);
        let c2 = anchor.col.max(end.col);
        let r2 = anchor.row.max(end.row);
        (c1, r1, c2, r2)
    }

    /// Check if a cell is within bounds (c1, r1) to (c2, r2).
    pub fn is_in_bounds(cell: &CellRef, c1: usize, r1: usize, c2: usize, r2: usize) -> bool {
        cell.row >= r1 && cell.row <= r2 && cell.col >= c1 && cell.col <= c2
    }
}
