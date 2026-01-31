use directories::ProjectDirs;
use gridline_engine::engine::{
    AST, Cell, CellRef, ComputedMap, Grid, SpillMap, create_engine_with_functions_and_spill,
};
use rhai::Engine;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

/// Maximum number of undo entries to keep
pub(crate) const MAX_UNDO_STACK: usize = 100;

/// Represents an undoable action
#[derive(Clone)]
pub struct UndoAction {
    pub cell_ref: CellRef,
    pub old_cell: Option<Cell>,
    pub new_cell: Option<Cell>,
}

/// UI-agnostic core state for the spreadsheet.
pub struct Core {
    /// The spreadsheet grid
    pub grid: Arc<Grid>,
    /// Rhai engine for evaluating formulas
    pub engine: Engine,
    /// Current file path
    pub file_path: Option<PathBuf>,
    /// Whether the grid has been modified
    pub modified: bool,
    /// Status message to display
    pub status_message: String,
    /// Paths to custom Rhai functions files
    pub functions_files: Vec<PathBuf>,
    /// Cached custom functions script content (concatenated from all files)
    pub custom_functions: Option<String>,
    /// Compiled custom functions AST
    pub custom_ast: Option<AST>,
    /// Reverse dependency map: cell -> cells that depend on it
    pub dependents: HashMap<CellRef, HashSet<CellRef>>,
    /// Maps spill cell positions to their source cell
    pub spill_sources: HashMap<CellRef, CellRef>,
    /// Shared spill map for computed spill values (accessible by engine builtins)
    pub spill_map: Arc<SpillMap>,
    /// Shared computed map for formula cell values (accessible by engine builtins)
    pub computed_map: Arc<ComputedMap>,
    /// Undo stack
    pub undo_stack: Vec<UndoAction>,
    /// Redo stack
    pub redo_stack: Vec<UndoAction>,
}

impl Core {
    /// Create a new core state
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

        let mut core = Core {
            grid,
            engine,
            file_path: None,
            modified: false,
            status_message: String::new(),
            functions_files: Vec::new(),
            custom_functions: None,
            custom_ast: None,
            dependents: HashMap::new(),
            spill_sources: HashMap::new(),
            spill_map,
            computed_map,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        };

        core.load_default_functions();
        core
    }

    /// Create core and load file if provided
    pub fn with_file(path: Option<PathBuf>, functions_files: Vec<PathBuf>) -> Result<Self> {
        let mut core = Self::new();

        // Load custom functions if specified
        for func_path in &functions_files {
            core.load_functions(func_path);
        }

        if let Some(ref p) = path {
            if p.exists() {
                core.load_file(p)?;
            } else {
                core.file_path = Some(p.clone());
                core.modified = false;
                core.status_message = format!("New file {}", p.display());
            }
        }
        Ok(core)
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

    /// Recreate the Rhai engine with current grid and custom functions
    pub(crate) fn recreate_engine(&mut self) {
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
    pub(crate) fn rebuild_dependents(&mut self) {
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
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}
use crate::error::Result;
