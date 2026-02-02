//! Application state and logic.
//!
//! This module contains the main [`App`] struct which holds all application state
//! including the spreadsheet grid, cursor position, editing buffers, and UI state.
//! The app operates in different [`Mode`]s (Normal, Edit, Command, Visual) similar
//! to Vim's modal editing.

use gridline_core::{Document, Result, ScriptContext};
use gridline_engine::engine::{Cell, CellRef};
use gridline_engine::plot::{PlotSpec, parse_plot_spec};
use std::collections::HashMap;
use std::path::PathBuf;

use super::keymap::Keymap;

/// Clipboard contents for yank/paste
#[derive(Clone)]
pub struct Clipboard {
    /// Cells stored as (relative_col, relative_row, cell)
    /// Position is relative to top-left of selection
    pub cells: Vec<(usize, usize, Cell)>,

    /// Original selection dimensions (kept for potential future paste-repeat feature)
    #[allow(dead_code)]
    pub width: usize,
    #[allow(dead_code)]
    pub height: usize,
}

/// Modal editing state for the application.
///
/// Similar to Vim, the application operates in different modes:
/// - [`Normal`](Mode::Normal): Navigate and execute commands
/// - [`Edit`](Mode::Edit): Edit cell contents
/// - [`Command`](Mode::Command): Enter ex-style commands (`:w`, `:q`, etc.)
/// - [`Visual`](Mode::Visual): Select cell ranges
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Navigate the grid, execute single-key commands.
    Normal,
    /// Edit the contents of the current cell.
    Edit,
    /// Enter ex-style commands (`:w`, `:q`, `:wq`, etc.).
    Command,
    /// Select a range of cells for yank/paste operations.
    Visual,
}

/// Main application state container.
///
/// Holds all state for the spreadsheet application including:
/// - The cell grid and Rhai evaluation engine
/// - Cursor position and viewport
/// - Editing and command buffers
/// - Undo/redo history
/// - Clipboard for yank/paste
/// - Modal UI state (plot, help)
pub struct App {
    /// Core spreadsheet state (UI-agnostic)
    pub core: Document,
    /// Current cursor position (column)
    pub cursor_col: usize,
    /// Current cursor position (row)
    pub cursor_row: usize,
    /// Viewport offset (column)
    pub viewport_col: usize,
    /// Viewport offset (row)
    pub viewport_row: usize,
    /// Number of visible columns
    pub visible_cols: usize,
    /// Number of visible rows
    pub visible_rows: usize,
    /// Maximum columns in spreadsheet
    pub max_cols: usize,
    /// Maximum rows in spreadsheet
    pub max_rows: usize,
    /// Current mode
    pub mode: Mode,
    /// Edit buffer for cell editing
    pub edit_buffer: String,
    /// Cursor position within edit buffer (byte offset)
    pub edit_cursor: usize,
    /// Command buffer for command mode
    pub command_buffer: String,
    /// Cursor position within command buffer (byte offset)
    pub command_cursor: usize,
    /// Column width for display
    pub col_width: usize,
    /// Visual mode selection anchor (col, row)
    pub selection_anchor: Option<(usize, usize)>,
    /// Clipboard for yank/paste
    pub clipboard: Option<Clipboard>,
    /// Per-column widths (column index -> width). Default is col_width.
    pub column_widths: HashMap<usize, usize>,
    /// Plot modal state (when open)
    pub plot_modal: Option<PlotSpec>,

    /// Help modal state
    pub help_modal: bool,

    /// Active keymap
    pub keymap: Keymap,

    /// Status message to display
    pub status_message: String,
}

impl App {
    /// Create a new application
    pub fn new() -> Self {
        let core = Document::new();

        App {
            core,
            cursor_col: 0,
            cursor_row: 0,
            viewport_col: 0,
            viewport_row: 0,
            visible_cols: 10,
            visible_rows: 20,
            max_cols: 26, // A-Z
            max_rows: 1000,
            mode: Mode::Normal,
            edit_buffer: String::new(),
            edit_cursor: 0,
            command_buffer: String::new(),
            command_cursor: 0,
            col_width: 12,
            selection_anchor: None,
            clipboard: None,
            column_widths: HashMap::new(),
            plot_modal: None,
            help_modal: false,
            keymap: Keymap::Vim,
            status_message: String::new(),
        }
    }

    pub fn close_plot_modal(&mut self) {
        self.plot_modal = None;
    }

    pub fn close_help_modal(&mut self) {
        self.help_modal = false;
    }

    pub fn open_plot_modal_at_cursor(&mut self) {
        let cell_ref = self.current_cell_ref();
        let display = self.core.get_cell_display(&cell_ref);
        if let Some(spec) = parse_plot_spec(&display) {
            self.plot_modal = Some(spec);
            self.status_message.clear();
        } else {
            self.status_message = "Error: Not a plot cell".to_string();
        }
    }

    /// Create app and load file if provided
    pub fn with_file(
        path: Option<PathBuf>,
        functions_files: Vec<PathBuf>,
        keymap: Keymap,
    ) -> Result<Self> {
        let mut app = Self::new();
        app.keymap = keymap;
        app.core = Document::with_file(path, functions_files)?;
        Ok(app)
    }

    /// Create a new application with an existing Document instance
    pub fn new_with_core(core: Document, keymap: Keymap) -> Self {
        let mut app = Self::new();
        app.core = core;
        app.keymap = keymap;
        app
    }

    /// Load custom Rhai functions from a file (appends to existing functions)
    pub fn load_functions(&mut self, path: &std::path::Path) {
        match self.core.load_functions(path) {
            Ok(p) => self.status_message = format!("Loaded functions from {}", p.display()),
            Err(e) => self.status_message = format!("Error: {}", e),
        }
    }

    /// Reload all custom functions from the loaded files
    pub fn reload_functions(&mut self) {
        match self.core.reload_functions() {
            Ok(count) => self.status_message = format!("Reloaded {} function file(s)", count),
            Err(e) => self.status_message = format!("Error: {}", e),
        }
    }

    /// Get the current cell reference
    pub fn current_cell_ref(&self) -> CellRef {
        CellRef::new(self.cursor_col, self.cursor_row)
    }

    /// Move cursor by delta, clamping to valid range
    pub fn move_cursor(&mut self, dx: i32, dy: i32) {
        self.cursor_col = (self.cursor_col as i32 + dx)
            .max(0)
            .min(self.max_cols as i32 - 1) as usize;
        self.cursor_row = (self.cursor_row as i32 + dy)
            .max(0)
            .min(self.max_rows as i32 - 1) as usize;
        self.update_viewport();
    }

    /// Update viewport to keep cursor visible
    pub fn update_viewport(&mut self) {
        // Horizontal scrolling
        if self.cursor_col < self.viewport_col {
            self.viewport_col = self.cursor_col;
        } else if self.cursor_col >= self.viewport_col + self.visible_cols {
            self.viewport_col = self.cursor_col - self.visible_cols + 1;
        }

        // Vertical scrolling
        if self.cursor_row < self.viewport_row {
            self.viewport_row = self.cursor_row;
        } else if self.cursor_row >= self.viewport_row + self.visible_rows {
            self.viewport_row = self.cursor_row - self.visible_rows + 1;
        }
    }

    /// Enter edit mode for current cell
    pub fn enter_edit_mode(&mut self) {
        let cell_ref = self.current_cell_ref();
        self.edit_buffer = if let Some(cell) = self.core.grid.get(&cell_ref) {
            cell.to_input_string()
        } else {
            String::new()
        };
        self.edit_cursor = self.edit_buffer.len(); // Cursor at end
        self.mode = Mode::Edit;
    }

    /// Commit the current edit
    pub fn commit_edit(&mut self) {
        let cell_ref = self.current_cell_ref();
        if let Err(e) = self.core.set_cell_from_input(cell_ref, &self.edit_buffer) {
            self.status_message = format!("Error: {}", e);
        } else {
            self.status_message.clear();
        }
        self.mode = Mode::Normal;
        self.edit_buffer.clear();
        self.edit_cursor = 0;
    }

    /// Clear the current cell
    pub fn clear_current_cell(&mut self) {
        let cell_ref = self.current_cell_ref();
        self.core.clear_cell(&cell_ref);
    }

    /// Insert a row above the cursor position
    pub fn insert_row(&mut self) {
        let at_row = self.cursor_row;
        self.core.insert_row(at_row);
        self.status_message = format!("Inserted row at {}", at_row + 1);
    }

    /// Delete the current row
    pub fn delete_row(&mut self) {
        let at_row = self.cursor_row;
        self.core.delete_row(at_row);
        self.status_message = format!("Deleted row {}", at_row + 1);
    }

    /// Insert a column left of the cursor position
    pub fn insert_column(&mut self) {
        let at_col = self.cursor_col;
        self.core.insert_column(at_col);
        self.status_message = format!("Inserted column at {}", CellRef::col_to_letters(at_col));

        // Shift column widths (UI state)
        let widths_to_shift: Vec<(usize, usize)> = self
            .column_widths
            .iter()
            .filter(|&(&col, _)| col >= at_col)
            .map(|(&col, &width)| (col, width))
            .collect();

        for (col, _) in &widths_to_shift {
            self.column_widths.remove(col);
        }
        for (col, width) in widths_to_shift {
            self.column_widths.insert(col + 1, width);
        }
    }

    /// Delete the current column
    pub fn delete_column(&mut self) {
        let at_col = self.cursor_col;
        self.core.delete_column(at_col);
        self.status_message = format!("Deleted column {}", CellRef::col_to_letters(at_col));

        // Shift column widths (UI state)
        self.column_widths.remove(&at_col);
        let widths_to_shift: Vec<(usize, usize)> = self
            .column_widths
            .iter()
            .filter(|&(&col, _)| col > at_col)
            .map(|(&col, &width)| (col, width))
            .collect();

        for (col, _) in &widths_to_shift {
            self.column_widths.remove(col);
        }
        for (col, width) in widths_to_shift {
            self.column_widths.insert(col - 1, width);
        }
    }

    /// Undo the last action
    pub fn undo(&mut self) {
        match self.core.undo() {
            Ok(()) => self.status_message = "Undone".to_string(),
            Err(e) => self.status_message = e.to_string(),
        }
    }

    /// Redo the last undone action
    pub fn redo(&mut self) {
        match self.core.redo() {
            Ok(()) => self.status_message = "Redone".to_string(),
            Err(e) => self.status_message = e.to_string(),
        }
    }

    /// Enter visual mode, anchoring selection at current cursor
    pub fn enter_visual_mode(&mut self) {
        self.selection_anchor = Some((self.cursor_col, self.cursor_row));
        self.mode = Mode::Visual;
        self.status_message = "-- VISUAL --".to_string();
    }

    /// Exit visual mode
    pub fn exit_visual_mode(&mut self) {
        self.selection_anchor = None;
        self.mode = Mode::Normal;
        self.status_message.clear();
    }

    /// Get current selection bounds (top_left, bottom_right) if in visual mode
    pub fn get_selection(&self) -> Option<((usize, usize), (usize, usize))> {
        let (anchor_col, anchor_row) = self.selection_anchor?;
        let min_col = anchor_col.min(self.cursor_col);
        let max_col = anchor_col.max(self.cursor_col);
        let min_row = anchor_row.min(self.cursor_row);
        let max_row = anchor_row.max(self.cursor_row);
        Some(((min_col, min_row), (max_col, max_row)))
    }

    /// Get the selection as a range string like "A1:B5"
    pub fn get_selection_range_string(&self) -> Option<String> {
        let ((c1, r1), (c2, r2)) = self.get_selection()?;
        let start = CellRef::new(c1, r1);
        let end = CellRef::new(c2, r2);
        Some(format!("{}:{}", start, end))
    }

    /// Yank current cell or selection to clipboard
    pub fn yank(&mut self) {
        let mut cells = Vec::new();

        if let Some(((c1, r1), (c2, r2))) = self.get_selection() {
            // Yank selection
            for row in r1..=r2 {
                for col in c1..=c2 {
                    let cell_ref = CellRef::new(col, row);

                    if let Some(cell) = self.core.grid.get(&cell_ref) {
                        // Normal cells: preserve original input/formula.
                        cells.push((col - c1, row - r1, cell.clone()));
                    } else if self.core.spill_sources.contains_key(&cell_ref)
                        || self.core.value_cache.contains_key(&cell_ref)
                    {
                        // Spill output cells are not stored in the grid; copy their evaluated value.
                        let display = self.core.get_cell_display(&cell_ref);
                        if !display.is_empty() {
                            let cell = gridline_engine::engine::Cell::from_input(&display);
                            cells.push((col - c1, row - r1, cell));
                        }
                    }
                }
            }
            let height = r2 - r1 + 1;
            let width = c2 - c1 + 1;
            self.clipboard = Some(Clipboard {
                cells,
                width,
                height,
            });
            self.status_message = format!("Yanked {}x{} cells", height, width);
            self.exit_visual_mode();
        } else {
            // Yank single cell
            let cell_ref = self.current_cell_ref();
            if let Some(cell) = self.core.grid.get(&cell_ref) {
                // Normal cells: preserve original input/formula.
                cells.push((0, 0, cell.clone()));
            } else if self.core.spill_sources.contains_key(&cell_ref)
                || self.core.value_cache.contains_key(&cell_ref)
            {
                // Spill output cell: copy evaluated value.
                let display = self.core.get_cell_display(&cell_ref);
                if !display.is_empty() {
                    let cell = gridline_engine::engine::Cell::from_input(&display);
                    cells.push((0, 0, cell));
                }
            }
            self.clipboard = Some(Clipboard {
                cells,
                width: 1,
                height: 1,
            });
            self.status_message = "Yanked cell".to_string();
        }
    }

    /// Paste clipboard at current cursor position
    pub fn paste(&mut self) {
        let Some(clipboard) = &self.clipboard else {
            self.status_message = "Nothing to paste".to_string();
            return;
        };

        let base_row = self.cursor_row;
        let base_col = self.cursor_col;
        let pasted = self.core.paste_cells(base_col, base_row, &clipboard.cells);

        self.status_message = format!("Pasted {} cells", pasted);
    }

    /// Get width for a specific column
    pub fn get_column_width(&self, col: usize) -> usize {
        *self.column_widths.get(&col).unwrap_or(&self.col_width)
    }

    /// Set width for current column
    pub fn set_column_width(&mut self, width: usize) {
        let width = width.clamp(4, 50); // Clamp to reasonable range
        self.column_widths.insert(self.cursor_col, width);
    }

    /// Increase current column width
    pub fn increase_column_width(&mut self) {
        let current = self.get_column_width(self.cursor_col);
        self.set_column_width(current + 2);
    }

    /// Decrease current column width
    pub fn decrease_column_width(&mut self) {
        let current = self.get_column_width(self.cursor_col);
        self.set_column_width(current.saturating_sub(2));
    }

    /// Jump to a specific cell reference
    pub fn goto_cell(&mut self, cell_ref_str: &str) {
        if let Some(cr) = CellRef::from_str(cell_ref_str) {
            if cr.col < self.max_cols && cr.row < self.max_rows {
                self.cursor_col = cr.col;
                self.cursor_row = cr.row;
                self.update_viewport();
                self.status_message = format!("Jumped to {}", cell_ref_str.to_uppercase());
            } else {
                self.status_message = "Cell out of range".to_string();
            }
        } else {
            self.status_message = format!("Invalid cell reference: {}", cell_ref_str);
        }
    }

    /// Go to the first cell (A1) - kept for potential `gg` keybinding
    #[allow(dead_code)]
    pub fn goto_first(&mut self) {
        self.cursor_col = 0;
        self.cursor_row = 0;
        self.update_viewport();
    }

    /// Go to the last row with data in the current column, or last row if no data
    pub fn goto_last(&mut self) {
        // Find the last row with data in any column
        let mut last_row = 0;
        for entry in self.core.grid.iter() {
            if entry.key().row > last_row {
                last_row = entry.key().row;
            }
        }
        self.cursor_row = last_row;
        self.update_viewport();
    }

    /// Execute a command entered in command mode.
    ///
    /// Returns `true` if the application should quit, `false` otherwise.
    pub fn execute_command(&mut self) -> bool {
        let cmd = self.command_buffer.trim().to_string();
        self.command_buffer.clear();
        self.command_cursor = 0;
        self.mode = Mode::Normal;

        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        let command = parts[0];
        let args = parts.get(1).map(|s| s.trim());

        match command {
            "q" => {
                if self.core.modified {
                    self.status_message =
                        "Unsaved changes! Use :q! to force quit or :wq to save and quit"
                            .to_string();
                    return false;
                }
                return true;
            }
            "q!" => {
                return true;
            }
            "w" | "save" => {
                if let Some(path) = args {
                    self.core.file_path = Some(PathBuf::from(path));
                }
                self.save_file();
            }
            "wq" => {
                self.save_file();
                if !self.core.modified {
                    return true;
                }
            }
            "e" | "open" | "load" => {
                if let Some(path) = args {
                    match self.core.load_file(&PathBuf::from(path)) {
                        Ok(()) => self.status_message = format!("Loaded {}", path),
                        Err(e) => self.status_message = format!("Error: {}", e),
                    }
                } else {
                    self.status_message = "Usage: :e <path>".to_string();
                }
            }
            "goto" | "g" => {
                if let Some(cell_ref) = args {
                    self.goto_cell(cell_ref);
                } else {
                    self.status_message = "Usage: :goto CELL (e.g., :goto A100)".to_string();
                }
            }
            "source" | "so" => {
                if let Some(path) = args {
                    self.load_functions(&PathBuf::from(path));
                } else if !self.core.functions_files.is_empty() {
                    self.reload_functions();
                } else {
                    self.status_message =
                        "Usage: :source <file.rhai> (or :so to reload current)".to_string();
                }
            }
            "colwidth" | "cw" => {
                if let Some(args) = args {
                    let parts: Vec<&str> = args.split_whitespace().collect();
                    match parts.len() {
                        1 => {
                            // :colwidth 15 - set current column
                            if let Ok(w) = parts[0].parse() {
                                self.set_column_width(w);
                                self.status_message = format!(
                                    "Column width set to {}",
                                    self.get_column_width(self.cursor_col)
                                );
                            } else {
                                self.status_message = "Invalid width".to_string();
                            }
                        }
                        2 => {
                            // :colwidth A 15 - set specific column
                            if let Some(col) = parse_column_letter(parts[0]) {
                                if let Ok(w) = parts[1].parse::<usize>() {
                                    let w = w.clamp(4, 50);
                                    self.column_widths.insert(col, w);
                                    self.status_message = format!(
                                        "Column {} width set to {}",
                                        CellRef::col_to_letters(col),
                                        w
                                    );
                                } else {
                                    self.status_message = "Invalid width".to_string();
                                }
                            } else {
                                self.status_message = "Invalid column".to_string();
                            }
                        }
                        _ => {
                            self.status_message = "Usage: :colwidth [COL] WIDTH".to_string();
                        }
                    }
                } else {
                    self.status_message = "Usage: :colwidth [COL] WIDTH".to_string();
                }
            }
            "import" => {
                if let Some(path) = args {
                    self.import_csv(path);
                } else {
                    self.status_message = "Usage: :import <file.csv>".to_string();
                }
            }
            "export" => {
                if let Some(path) = args {
                    self.export_csv(path);
                } else {
                    self.status_message = "Usage: :export <file.csv>".to_string();
                }
            }
            "ir" | "insertrow" => {
                self.insert_row();
            }
            "dr" | "deleterow" => {
                self.delete_row();
            }
            "ic" | "insertcol" => {
                self.insert_column();
            }
            "dc" | "deletecol" => {
                self.delete_column();
            }
            "help" | "h" => {
                self.help_modal = true;
            }
            "call" => {
                // :call func_name(args) - Execute a function from custom Rhai script
                if let Some(expr) = args {
                    self.execute_rhai_script(expr);
                } else {
                    self.status_message = "Usage: :call func_name(args)".to_string();
                }
            }
            "rhai" => {
                // :rhai expression - Execute arbitrary Rhai expression
                if let Some(expr) = args {
                    self.execute_rhai_script(expr);
                } else {
                    self.status_message = "Usage: :rhai <expression>".to_string();
                }
            }
            _ => {
                self.status_message = format!("Unknown command: {}", command);
            }
        }
        false
    }

    /// Execute a Rhai script with access to spreadsheet write operations.
    fn execute_rhai_script(&mut self, script: &str) {
        // Build script context with cursor position and selection
        let context = if let Some(((c1, r1), (c2, r2))) = self.get_selection() {
            ScriptContext::with_selection(self.cursor_col, self.cursor_row, c1, r1, c2, r2)
        } else {
            ScriptContext::new(self.cursor_col, self.cursor_row)
        };

        match self.core.execute_script(script, &context) {
            Ok(result) => {
                let mut msg = format!("{} cell(s) modified", result.cells_modified);
                if let Some(val) = result.return_value {
                    msg.push_str(&format!(" => {}", val));
                }
                self.status_message = msg;
                // Exit visual mode after script execution
                if self.mode == Mode::Visual {
                    self.exit_visual_mode();
                }
            }
            Err(e) => {
                self.status_message = format!("Script error: {}", e);
            }
        }
    }

    /// Save to current file path
    pub fn save_file(&mut self) {
        match self.core.save_file() {
            Ok(path) => self.status_message = format!("Saved to {}", path.display()),
            Err(e) => self.status_message = format!("Error: {}", e),
        }
    }

    /// Import CSV data starting at current cursor position
    fn import_csv(&mut self, path: &str) {
        match self.core.import_csv(path, self.cursor_col, self.cursor_row) {
            Ok(count) => self.status_message = format!("Imported {} cells from {}", count, path),
            Err(e) => self.status_message = format!("Error: {}", e),
        }
    }

    /// Export grid to CSV file
    fn export_csv(&mut self, path: &str) {
        match self.core.export_csv(path, self.get_selection()) {
            Ok(()) => self.status_message = format!("Exported to {}", path),
            Err(e) => self.status_message = format!("Error: {}", e),
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse column letter(s) to column index (e.g., "A" -> 0, "AA" -> 26)
fn parse_column_letter(s: &str) -> Option<usize> {
    let s = s.trim().to_uppercase();
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let col = s
        .bytes()
        .fold(0usize, |acc, c| acc * 26 + (c - b'A') as usize + 1)
        - 1;
    Some(col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridline_engine::engine::CellType;

    #[test]
    fn test_get_selection_col_row_order() {
        let mut app = App::new();
        app.cursor_col = 3;
        app.cursor_row = 4;
        app.selection_anchor = Some((1, 1));

        let selection = app.get_selection().unwrap();
        assert_eq!(selection, ((1, 1), (3, 4)));
        assert_eq!(app.get_selection_range_string().unwrap(), "B2:D5");
    }

    #[test]
    fn test_paste_uses_col_row_coordinates() {
        let mut app = App::new();
        app.core
            .set_cell_from_input(CellRef::new(1, 2), "42")
            .unwrap();
        app.cursor_col = 1;
        app.cursor_row = 2;
        app.yank();

        app.cursor_col = 3;
        app.cursor_row = 0;
        app.paste();

        let cell = app.core.grid.get(&CellRef::new(3, 0)).unwrap();
        assert!(matches!(
            cell.contents,
            CellType::Number(n) if (n - 42.0).abs() < 0.001
        ));
    }
}
