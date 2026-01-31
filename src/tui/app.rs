//! Application state and logic.
//!
//! This module contains the main [`App`] struct which holds all application state
//! including the spreadsheet grid, cursor position, editing buffers, and UI state.
//! The app operates in different [`Mode`]s (Normal, Edit, Command, Visual) similar
//! to Vim's modal editing.

use crate::error::Result;
use crate::storage::{parse_csv, parse_grd, write_csv, write_grd};
use gridline_engine::engine::{
    AST, Cell, CellRef, CellType, ComputedMap, Grid, ShiftOperation, SpillMap,
    create_engine_with_functions_and_spill, detect_cycle, eval_with_functions, format_dynamic,
    format_number, preprocess_script_with_context, shift_formula_references,
};
use gridline_engine::plot::{PlotSpec, parse_plot_spec};
use rhai::{Dynamic, Engine};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use directories::ProjectDirs;

use super::keymap::Keymap;

/// Maximum number of undo entries to keep
const MAX_UNDO_STACK: usize = 100;

/// Represents an undoable action
#[derive(Clone)]
pub struct UndoAction {
    pub cell_ref: CellRef,
    pub old_cell: Option<Cell>,
    pub new_cell: Option<Cell>,
}

/// Clipboard contents for yank/paste
#[derive(Clone)]
pub struct Clipboard {
    /// Cells stored as (relative_row, relative_col, cell)
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
    /// The spreadsheet grid
    pub grid: Arc<Grid>,
    /// Rhai engine for evaluating formulas
    pub engine: Engine,
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
    /// Current file path
    pub file_path: Option<PathBuf>,
    /// Whether the grid has been modified
    pub modified: bool,
    /// Status message to display
    pub status_message: String,
    /// Confirm quit flag
    pub confirm_quit: bool,
    /// Column width for display
    pub col_width: usize,
    /// Undo stack
    pub undo_stack: Vec<UndoAction>,
    /// Redo stack
    pub redo_stack: Vec<UndoAction>,
    /// Visual mode selection anchor (where selection started)
    pub selection_anchor: Option<(usize, usize)>,
    /// Clipboard for yank/paste
    pub clipboard: Option<Clipboard>,
    /// Per-column widths (column index -> width). Default is col_width.
    pub column_widths: HashMap<usize, usize>,
    /// Paths to custom Rhai functions files
    pub functions_files: Vec<PathBuf>,
    /// Cached custom functions script content (concatenated from all files)
    pub custom_functions: Option<String>,
    /// Compiled custom functions AST
    pub custom_ast: Option<AST>,
    /// Reverse dependency map: cell -> cells that depend on it
    pub dependents: HashMap<CellRef, HashSet<CellRef>>,

    /// Plot modal state (when open)
    pub plot_modal: Option<PlotSpec>,

    /// Help modal state
    pub help_modal: bool,

    /// Active keymap
    pub keymap: Keymap,

    /// Maps spill cell positions to their source cell
    pub spill_sources: HashMap<CellRef, CellRef>,

    /// Shared spill map for computed spill values (accessible by engine builtins)
    pub spill_map: Arc<SpillMap>,

    /// Shared computed map for formula cell values (accessible by engine builtins)
    pub computed_map: Arc<ComputedMap>,
}

impl App {
    /// Create a new application
    pub fn new() -> Self {
        let grid = Arc::new(Grid::new());
        let spill_map = Arc::new(SpillMap::new());
        let computed_map = Arc::new(ComputedMap::new());
        let (engine, _, _) = create_engine_with_functions_and_spill(
            Arc::clone(&grid),
            Arc::clone(&spill_map),
            Arc::clone(&computed_map),
            None,
        );

        let mut app = App {
            grid,
            engine,
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
            file_path: None,
            modified: false,
            status_message: String::new(),
            confirm_quit: false,
            col_width: 12,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            selection_anchor: None,
            clipboard: None,
            column_widths: HashMap::new(),
            functions_files: Vec::new(),
            custom_functions: None,
            custom_ast: None,
            dependents: HashMap::new(),
            plot_modal: None,
            help_modal: false,
            keymap: Keymap::Vim,
            spill_sources: HashMap::new(),
            spill_map,
            computed_map,
        };

        app.load_default_functions();
        app
    }

    pub fn close_plot_modal(&mut self) {
        self.plot_modal = None;
    }

    pub fn close_help_modal(&mut self) {
        self.help_modal = false;
    }

    pub fn open_plot_modal_at_cursor(&mut self) {
        let cell_ref = self.current_cell_ref();
        let display = self.get_cell_display(&cell_ref);
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

        // Load custom functions if specified
        for func_path in &functions_files {
            app.load_functions(func_path);
        }

        if let Some(ref p) = path {
            app.load_file(p)?;
        }
        Ok(app)
    }

    fn load_default_functions(&mut self) {
        let Some(proj) = ProjectDirs::from("", "", "gridline") else {
            return;
        };
        let mut path = proj.config_dir().to_path_buf();
        path.push("default.rhai");
        if path.exists() {
            self.load_functions_silent(&path);
        }
    }

    fn load_functions_silent(&mut self, path: &std::path::Path) {
        match fs::read_to_string(path) {
            Ok(content) => {
                let path_buf = path.to_path_buf();
                if !self.functions_files.contains(&path_buf) {
                    self.functions_files.push(path_buf);
                }
                match &mut self.custom_functions {
                    Some(existing) => {
                        existing.push_str("\n\n");
                        existing.push_str(&content);
                    }
                    None => {
                        self.custom_functions = Some(content);
                    }
                }
                self.recreate_engine();
            }
            Err(e) => {
                self.status_message = format!("Error loading functions: {}", e);
            }
        }
    }

    /// Load custom Rhai functions from a file (appends to existing functions)
    pub fn load_functions(&mut self, path: &std::path::Path) {
        match fs::read_to_string(path) {
            Ok(content) => {
                // Add to list if not already present
                let path_buf = path.to_path_buf();
                if !self.functions_files.contains(&path_buf) {
                    self.functions_files.push(path_buf);
                }
                // Concatenate with existing functions
                match &mut self.custom_functions {
                    Some(existing) => {
                        existing.push_str("\n\n");
                        existing.push_str(&content);
                    }
                    None => {
                        self.custom_functions = Some(content);
                    }
                }
                self.recreate_engine();
                if self.status_message.starts_with("Error") {
                    // Error message already set by recreate_engine
                } else {
                    self.status_message = format!("Loaded functions from {}", path.display());
                }
            }
            Err(e) => {
                self.status_message = format!("Error loading functions: {}", e);
            }
        }
    }

    /// Reload all custom functions from the loaded files
    pub fn reload_functions(&mut self) {
        if self.functions_files.is_empty() {
            self.status_message = "No functions file loaded".to_string();
            return;
        }
        // Re-read all files
        let paths = self.functions_files.clone();
        self.functions_files.clear();
        self.custom_functions = None;
        for path in paths {
            self.load_functions(&path);
        }
    }

    /// Recreate the Rhai engine with current grid and custom functions
    fn recreate_engine(&mut self) {
        let (engine, ast, error) = create_engine_with_functions_and_spill(
            Arc::clone(&self.grid),
            Arc::clone(&self.spill_map),
            Arc::clone(&self.computed_map),
            self.custom_functions.as_deref(),
        );
        self.engine = engine;
        self.custom_ast = ast;
        if let Some(err) = error {
            self.status_message = err;
        }
        self.rebuild_dependents();
    }

    /// Rebuild the reverse dependency map from the grid
    fn rebuild_dependents(&mut self) {
        self.dependents.clear();
        for entry in self.grid.iter() {
            let cell_ref = entry.key();
            let cell = entry.value();
            for dep in &cell.depends_on {
                self.dependents
                    .entry(dep.clone())
                    .or_default()
                    .insert(cell_ref.clone());
            }
        }
    }

    /// Mark all cells that depend (transitively) on the changed cell as dirty
    fn mark_dependents_dirty(&mut self, changed_cell: &CellRef) {
        let mut to_process = vec![changed_cell.clone()];
        let mut visited = HashSet::new();
        let mut spills_to_clear = Vec::new();

        while let Some(cell_ref) = to_process.pop() {
            if !visited.insert(cell_ref.clone()) {
                continue;
            }

            // Mark all cells that depend on this one as dirty
            if let Some(deps) = self.dependents.get(&cell_ref) {
                for dep in deps.clone() {
                    if let Some(mut cell) = self.grid.get_mut(&dep) {
                        cell.dirty = true;
                        cell.cached_value = None;
                    }
                    // Clear computed value so it will be re-evaluated
                    self.computed_map.remove(&dep);
                    // Track spills that need clearing
                    if self.spill_map.contains_key(&dep) {
                        spills_to_clear.push(dep.clone());
                    }
                    to_process.push(dep.clone());
                }
            }
        }

        // Clear spills from dirty cells
        for source in spills_to_clear {
            self.clear_spill_from(&source);
        }
    }

    /// Get the current cell reference
    pub fn current_cell_ref(&self) -> CellRef {
        CellRef::new(self.cursor_row, self.cursor_col)
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
        self.edit_buffer = if let Some(cell) = self.grid.get(&cell_ref) {
            cell.to_input_string()
        } else {
            String::new()
        };
        self.edit_cursor = self.edit_buffer.len(); // Cursor at end
        self.mode = Mode::Edit;
    }

    /// Push an undo action before modifying a cell
    fn push_undo(&mut self, cell_ref: CellRef, new_cell: Option<Cell>) {
        let old_cell = self.grid.get(&cell_ref).map(|r| r.clone());
        self.undo_stack.push(UndoAction {
            cell_ref,
            old_cell,
            new_cell,
        });
        self.redo_stack.clear();
        if self.undo_stack.len() > MAX_UNDO_STACK {
            self.undo_stack.remove(0);
        }
    }

    /// Commit the current edit
    pub fn commit_edit(&mut self) {
        let cell_ref = self.current_cell_ref();
        let cell = Cell::from_input(&self.edit_buffer);

        // Check for circular dependencies if it's a script
        if let CellType::Script(_) = &cell.contents {
            // Temporarily insert to check for cycles
            let old_cell = self.grid.get(&cell_ref).map(|r| r.clone());
            self.grid.insert(cell_ref.clone(), cell.clone());
            if let Some(_cycle) = detect_cycle(&cell_ref, &self.grid) {
                // Restore old state
                match old_cell {
                    Some(c) => {
                        self.grid.insert(cell_ref, c);
                    }
                    None => {
                        self.grid.remove(&cell_ref);
                    }
                }
                self.status_message = "Error: Circular dependency detected".to_string();
                self.mode = Mode::Normal;
                self.edit_buffer.clear();
                self.edit_cursor = 0;
                return;
            }
            // Cycle check passed, now push undo (restore old state first, then re-insert)
            match old_cell {
                Some(ref c) => {
                    self.grid.insert(cell_ref.clone(), c.clone());
                }
                None => {
                    self.grid.remove(&cell_ref);
                }
            }
            self.push_undo(cell_ref.clone(), Some(cell.clone()));
            self.grid.insert(cell_ref.clone(), cell);
        } else {
            self.push_undo(cell_ref.clone(), Some(cell.clone()));
            self.grid.insert(cell_ref.clone(), cell);
        }

        self.modified = true;
        self.mode = Mode::Normal;
        self.edit_buffer.clear();
        self.edit_cursor = 0;
        self.status_message.clear();

        // Clear any spill originating from this cell
        self.clear_spill_from(&cell_ref);

        // Recreate engine with updated grid
        self.recreate_engine();

        // Mark dependent cells as dirty
        self.mark_dependents_dirty(&cell_ref);
    }

    /// Clear the current cell
    pub fn clear_current_cell(&mut self) {
        let cell_ref = self.current_cell_ref();
        if self.grid.get(&cell_ref).is_some() {
            self.push_undo(cell_ref.clone(), None);
            self.grid.remove(&cell_ref);
            self.modified = true;

            // Clear any spill originating from this cell
            self.clear_spill_from(&cell_ref);

            self.recreate_engine();
            self.mark_dependents_dirty(&cell_ref);
        }
    }

    /// Insert a row above the cursor position
    pub fn insert_row(&mut self) {
        let at_row = self.cursor_row;

        // Collect all cells at row >= at_row
        let cells_to_move: Vec<(CellRef, Cell)> = self
            .grid
            .iter()
            .filter(|entry| entry.key().row >= at_row)
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        // Remove them from grid
        for (cell_ref, _) in &cells_to_move {
            self.grid.remove(cell_ref);
        }

        // Update ALL formulas in the grid with shifted references
        let op = ShiftOperation::InsertRow(at_row);
        let all_cells: Vec<(CellRef, Cell)> = self
            .grid
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        for (cell_ref, cell) in all_cells {
            if let CellType::Script(formula) = &cell.contents {
                let new_formula = shift_formula_references(formula, op);
                if new_formula != *formula {
                    let new_cell = Cell::new_script(&new_formula);
                    self.grid.insert(cell_ref, new_cell);
                }
            }
        }

        // Reinsert moved cells with row + 1, also shifting their formulas
        for (cell_ref, cell) in cells_to_move {
            let new_ref = CellRef::new(cell_ref.row + 1, cell_ref.col);
            let new_cell = if let CellType::Script(formula) = &cell.contents {
                let new_formula = shift_formula_references(formula, op);
                Cell::new_script(&new_formula)
            } else {
                cell
            };
            self.grid.insert(new_ref, new_cell);
        }

        // Clear spill/computed maps and rebuild
        self.spill_sources.clear();
        self.spill_map.clear();
        self.computed_map.clear();
        self.recreate_engine();
        self.modified = true;
        self.status_message = format!("Inserted row at {}", at_row + 1);
    }

    /// Delete the current row
    pub fn delete_row(&mut self) {
        let at_row = self.cursor_row;

        // Collect cells at the deleted row (for undo - future enhancement)
        let cells_at_row: Vec<CellRef> = self
            .grid
            .iter()
            .filter(|entry| entry.key().row == at_row)
            .map(|entry| entry.key().clone())
            .collect();

        // Remove cells at the deleted row
        for cell_ref in cells_at_row {
            self.grid.remove(&cell_ref);
        }

        // Collect cells below the deleted row
        let cells_to_move: Vec<(CellRef, Cell)> = self
            .grid
            .iter()
            .filter(|entry| entry.key().row > at_row)
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        // Remove them from grid
        for (cell_ref, _) in &cells_to_move {
            self.grid.remove(cell_ref);
        }

        // Update ALL formulas with shifted references
        let op = ShiftOperation::DeleteRow(at_row);
        let all_cells: Vec<(CellRef, Cell)> = self
            .grid
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        for (cell_ref, cell) in all_cells {
            if let CellType::Script(formula) = &cell.contents {
                let new_formula = shift_formula_references(formula, op);
                if new_formula != *formula {
                    let new_cell = if new_formula.contains("#REF!") {
                        // Create a text cell with #REF! error
                        Cell::new_text(&format!("={}", new_formula))
                    } else {
                        Cell::new_script(&new_formula)
                    };
                    self.grid.insert(cell_ref, new_cell);
                }
            }
        }

        // Reinsert moved cells with row - 1, also shifting their formulas
        for (cell_ref, cell) in cells_to_move {
            let new_ref = CellRef::new(cell_ref.row - 1, cell_ref.col);
            let new_cell = if let CellType::Script(formula) = &cell.contents {
                let new_formula = shift_formula_references(formula, op);
                if new_formula.contains("#REF!") {
                    Cell::new_text(&format!("={}", new_formula))
                } else {
                    Cell::new_script(&new_formula)
                }
            } else {
                cell
            };
            self.grid.insert(new_ref, new_cell);
        }

        // Clear spill/computed maps and rebuild
        self.spill_sources.clear();
        self.spill_map.clear();
        self.computed_map.clear();
        self.recreate_engine();
        self.modified = true;
        self.status_message = format!("Deleted row {}", at_row + 1);
    }

    /// Insert a column left of the cursor position
    pub fn insert_column(&mut self) {
        let at_col = self.cursor_col;

        // Collect all cells at col >= at_col
        let cells_to_move: Vec<(CellRef, Cell)> = self
            .grid
            .iter()
            .filter(|entry| entry.key().col >= at_col)
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        // Remove them from grid
        for (cell_ref, _) in &cells_to_move {
            self.grid.remove(cell_ref);
        }

        // Update ALL formulas in the grid with shifted references
        let op = ShiftOperation::InsertColumn(at_col);
        let all_cells: Vec<(CellRef, Cell)> = self
            .grid
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        for (cell_ref, cell) in all_cells {
            if let CellType::Script(formula) = &cell.contents {
                let new_formula = shift_formula_references(formula, op);
                if new_formula != *formula {
                    let new_cell = Cell::new_script(&new_formula);
                    self.grid.insert(cell_ref, new_cell);
                }
            }
        }

        // Reinsert moved cells with col + 1, also shifting their formulas
        for (cell_ref, cell) in cells_to_move {
            let new_ref = CellRef::new(cell_ref.row, cell_ref.col + 1);
            let new_cell = if let CellType::Script(formula) = &cell.contents {
                let new_formula = shift_formula_references(formula, op);
                Cell::new_script(&new_formula)
            } else {
                cell
            };
            self.grid.insert(new_ref, new_cell);
        }

        // Shift column widths
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

        // Clear spill/computed maps and rebuild
        self.spill_sources.clear();
        self.spill_map.clear();
        self.computed_map.clear();
        self.recreate_engine();
        self.modified = true;
        self.status_message = format!("Inserted column at {}", CellRef::col_to_letters(at_col));
    }

    /// Delete the current column
    pub fn delete_column(&mut self) {
        let at_col = self.cursor_col;

        // Collect cells at the deleted column
        let cells_at_col: Vec<CellRef> = self
            .grid
            .iter()
            .filter(|entry| entry.key().col == at_col)
            .map(|entry| entry.key().clone())
            .collect();

        // Remove cells at the deleted column
        for cell_ref in cells_at_col {
            self.grid.remove(&cell_ref);
        }

        // Collect cells to the right of the deleted column
        let cells_to_move: Vec<(CellRef, Cell)> = self
            .grid
            .iter()
            .filter(|entry| entry.key().col > at_col)
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        // Remove them from grid
        for (cell_ref, _) in &cells_to_move {
            self.grid.remove(cell_ref);
        }

        // Update ALL formulas with shifted references
        let op = ShiftOperation::DeleteColumn(at_col);
        let all_cells: Vec<(CellRef, Cell)> = self
            .grid
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        for (cell_ref, cell) in all_cells {
            if let CellType::Script(formula) = &cell.contents {
                let new_formula = shift_formula_references(formula, op);
                if new_formula != *formula {
                    let new_cell = if new_formula.contains("#REF!") {
                        Cell::new_text(&format!("={}", new_formula))
                    } else {
                        Cell::new_script(&new_formula)
                    };
                    self.grid.insert(cell_ref, new_cell);
                }
            }
        }

        // Reinsert moved cells with col - 1, also shifting their formulas
        for (cell_ref, cell) in cells_to_move {
            let new_ref = CellRef::new(cell_ref.row, cell_ref.col - 1);
            let new_cell = if let CellType::Script(formula) = &cell.contents {
                let new_formula = shift_formula_references(formula, op);
                if new_formula.contains("#REF!") {
                    Cell::new_text(&format!("={}", new_formula))
                } else {
                    Cell::new_script(&new_formula)
                }
            } else {
                cell
            };
            self.grid.insert(new_ref, new_cell);
        }

        // Shift column widths
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

        // Clear spill/computed maps and rebuild
        self.spill_sources.clear();
        self.spill_map.clear();
        self.computed_map.clear();
        self.recreate_engine();
        self.modified = true;
        self.status_message = format!("Deleted column {}", CellRef::col_to_letters(at_col));
    }

    /// Undo the last action
    pub fn undo(&mut self) {
        let Some(action) = self.undo_stack.pop() else {
            self.status_message = "Nothing to undo".to_string();
            return;
        };

        // Push inverse to redo stack
        let current = self.grid.get(&action.cell_ref).map(|r| r.clone());
        self.redo_stack.push(UndoAction {
            cell_ref: action.cell_ref.clone(),
            old_cell: current,
            new_cell: action.old_cell.clone(),
        });

        let cell_ref = action.cell_ref.clone();

        // Restore old state
        match action.old_cell {
            Some(cell) => {
                self.grid.insert(action.cell_ref, cell);
            }
            None => {
                self.grid.remove(&action.cell_ref);
            }
        }

        self.recreate_engine();
        self.mark_dependents_dirty(&cell_ref);
        self.status_message = "Undone".to_string();
    }

    /// Redo the last undone action
    pub fn redo(&mut self) {
        let Some(action) = self.redo_stack.pop() else {
            self.status_message = "Nothing to redo".to_string();
            return;
        };

        // Push inverse to undo stack
        let current = self.grid.get(&action.cell_ref).map(|r| r.clone());
        self.undo_stack.push(UndoAction {
            cell_ref: action.cell_ref.clone(),
            old_cell: current,
            new_cell: action.new_cell.clone(),
        });

        let cell_ref = action.cell_ref.clone();

        // Apply new state
        match action.new_cell {
            Some(cell) => {
                self.grid.insert(action.cell_ref, cell);
            }
            None => {
                self.grid.remove(&action.cell_ref);
            }
        }

        self.recreate_engine();
        self.mark_dependents_dirty(&cell_ref);
        self.status_message = "Redone".to_string();
    }

    /// Enter visual mode, anchoring selection at current cursor
    pub fn enter_visual_mode(&mut self) {
        self.selection_anchor = Some((self.cursor_row, self.cursor_col));
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
        let (anchor_row, anchor_col) = self.selection_anchor?;
        let min_row = anchor_row.min(self.cursor_row);
        let max_row = anchor_row.max(self.cursor_row);
        let min_col = anchor_col.min(self.cursor_col);
        let max_col = anchor_col.max(self.cursor_col);
        Some(((min_row, min_col), (max_row, max_col)))
    }

    /// Get the selection as a range string like "A1:B5"
    pub fn get_selection_range_string(&self) -> Option<String> {
        let ((r1, c1), (r2, c2)) = self.get_selection()?;
        let start = CellRef::new(r1, c1);
        let end = CellRef::new(r2, c2);
        Some(format!("{}:{}", start, end))
    }

    /// Yank current cell or selection to clipboard
    pub fn yank(&mut self) {
        let mut cells = Vec::new();

        if let Some(((r1, c1), (r2, c2))) = self.get_selection() {
            // Yank selection
            for row in r1..=r2 {
                for col in c1..=c2 {
                    let cell_ref = CellRef::new(row, col);
                    if let Some(cell) = self.grid.get(&cell_ref) {
                        cells.push((row - r1, col - c1, cell.clone()));
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
            if let Some(cell) = self.grid.get(&cell_ref) {
                cells.push((0, 0, cell.clone()));
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
        let clipboard_cells = clipboard.cells.clone();

        let mut pasted_cells = Vec::new();
        for (rel_row, rel_col, cell) in &clipboard_cells {
            let target = CellRef::new(base_row + rel_row, base_col + rel_col);
            self.push_undo(target.clone(), Some(cell.clone()));
            self.grid.insert(target.clone(), cell.clone());
            pasted_cells.push(target);
        }

        self.modified = true;
        self.recreate_engine();

        // Mark dependents of all pasted cells as dirty
        for cell_ref in pasted_cells {
            self.mark_dependents_dirty(&cell_ref);
        }

        self.status_message = format!("Pasted {} cells", clipboard_cells.len());
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
        for entry in self.grid.iter() {
            if entry.key().row > last_row {
                last_row = entry.key().row;
            }
        }
        self.cursor_row = last_row;
        self.update_viewport();
    }

    /// Get the display value for a cell
    pub fn get_cell_display(&mut self, cell_ref: &CellRef) -> String {
        // Check if this is a spill cell (value is in shared spill_map)
        if self.spill_sources.contains_key(cell_ref) {
            if let Some(val) = self.spill_map.get(cell_ref) {
                return format_dynamic(&val);
            }
            return "#SPILL?".to_string(); // Orphaned spill cell
        }

        let Some(cell) = self.grid.get(cell_ref) else {
            // Also check spill_map for cells that aren't in spill_sources
            // (the source cell itself stores its first value there too)
            if let Some(val) = self.spill_map.get(cell_ref) {
                return format_dynamic(&val);
            }
            return String::new();
        };

        match &cell.contents {
            CellType::Empty => String::new(),
            CellType::Text(s) => s.clone(),
            CellType::Number(n) => format_number(*n),
            CellType::Script(s) => {
                // Return cached value if not dirty
                if !cell.dirty
                    && let Some(ref cached) = cell.cached_value
                {
                    return cached.clone();
                }

                // Check for cycles
                if detect_cycle(cell_ref, &self.grid).is_some() {
                    return "#CYCLE!".to_string();
                }

                let processed = preprocess_script_with_context(s, Some(cell_ref));
                drop(cell);

                match eval_with_functions(&self.engine, &processed, self.custom_ast.as_ref()) {
                    Ok(result) => {
                        if result.is_array() {
                            self.handle_array_spill(cell_ref, result)
                        } else {
                            // Store in computed_map so other formulas can reference this value
                            self.computed_map.insert(cell_ref.clone(), result.clone());
                            let display = format_dynamic(&result);
                            // Cache the result and clear dirty flag
                            if let Some(mut cell) = self.grid.get_mut(cell_ref) {
                                cell.cached_value = Some(display.clone());
                                cell.dirty = false;
                            }
                            display
                        }
                    }
                    Err(_) => "#ERR!".to_string(),
                }
            }
        }
    }

    /// Handle array result - check conflicts and set up spill
    fn handle_array_spill(&mut self, source: &CellRef, result: Dynamic) -> String {
        let array: Vec<Dynamic> = result.into_array().unwrap();
        if array.is_empty() {
            return String::new();
        }

        // Check for conflicts in spill range
        for i in 1..array.len() {
            let spill_ref = CellRef::new(source.row + i, source.col);

            // Conflict if cell exists with content
            if let Some(cell) = self.grid.get(&spill_ref) {
                if !matches!(cell.contents, CellType::Empty) {
                    return "#SPILL!".to_string();
                }
            }

            // Conflict if another formula is spilling here
            if let Some(other_source) = self.spill_sources.get(&spill_ref) {
                if other_source != source {
                    return "#SPILL!".to_string();
                }
            }
        }

        // Clear old spill from this source
        self.clear_spill_from(source);

        // Store all array values in the shared spill_map
        // This makes them accessible to the engine for chained VEC calls
        for (i, val) in array.iter().enumerate() {
            let cell_ref = CellRef::new(source.row + i, source.col);
            self.spill_map.insert(cell_ref.clone(), val.clone());

            // Register spill cells (skip index 0, that's the source cell)
            if i > 0 {
                self.spill_sources.insert(cell_ref, source.clone());
            }
        }

        // Format first value for display and cache
        let first = format_dynamic(&array[0]);

        // Cache the first value in the source cell
        if let Some(mut cell) = self.grid.get_mut(source) {
            cell.cached_value = Some(first.clone());
            cell.dirty = false;
        }

        first
    }

    /// Clear spill cells originating from a source
    fn clear_spill_from(&mut self, source: &CellRef) {
        // Remove the source cell's value from spill_map
        self.spill_map.remove(source);

        // Remove all spill cells from this source
        let to_remove: Vec<CellRef> = self
            .spill_sources
            .iter()
            .filter(|(_, src)| *src == source)
            .map(|(cell, _)| cell.clone())
            .collect();

        for cell in to_remove {
            self.spill_sources.remove(&cell);
            self.spill_map.remove(&cell);
        }
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
                if self.modified {
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
                    self.file_path = Some(PathBuf::from(path));
                }
                self.save_file();
            }
            "wq" => {
                self.save_file();
                if !self.modified {
                    return true;
                }
            }
            "e" | "open" => {
                if let Some(path) = args {
                    if let Err(e) = self.load_file(&PathBuf::from(path)) {
                        self.status_message = format!("Error: {}", e);
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
                } else if !self.functions_files.is_empty() {
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
                        _ => self.status_message = "Usage: :colwidth [COL] WIDTH".to_string(),
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
            _ => {
                self.status_message = format!("Unknown command: {}", command);
            }
        }
        false
    }

    /// Save to current file path
    pub fn save_file(&mut self) {
        let Some(path) = &self.file_path else {
            self.status_message = "No file path. Use :w <path>".to_string();
            return;
        };

        match write_grd(path, &self.grid) {
            Ok(()) => {
                self.modified = false;
                self.status_message = format!("Saved to {}", path.display());
            }
            Err(e) => {
                self.status_message = format!("Error saving: {}", e);
            }
        }
    }

    /// Load from file
    pub fn load_file(&mut self, path: &std::path::Path) -> Result<()> {
        let grid = parse_grd(path)?;
        self.grid = Arc::new(grid);
        self.recreate_engine();
        self.file_path = Some(path.to_path_buf());
        self.modified = false;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.status_message = format!("Loaded {}", path.display());
        Ok(())
    }

    /// Import CSV data starting at current cursor position
    fn import_csv(&mut self, path: &str) {
        match parse_csv(std::path::Path::new(path), self.cursor_row, self.cursor_col) {
            Ok(cells) => {
                let count = cells.len();
                if count == 0 {
                    self.status_message = "CSV file is empty".to_string();
                    return;
                }
                for (cell_ref, cell) in cells {
                    self.grid.insert(cell_ref, cell);
                }
                self.modified = true;
                self.recreate_engine();
                self.status_message = format!("Imported {} cells from {}", count, path);
            }
            Err(e) => {
                self.status_message = format!("Import error: {}", e);
            }
        }
    }

    /// Export grid to CSV file
    fn export_csv(&mut self, path: &str) {
        // Use visual selection if active, otherwise auto-detect data bounds
        let range = self.get_selection();
        match write_csv(std::path::Path::new(path), &self.grid, range) {
            Ok(()) => {
                self.status_message = format!("Exported to {}", path);
            }
            Err(e) => {
                self.status_message = format!("Export error: {}", e);
            }
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
