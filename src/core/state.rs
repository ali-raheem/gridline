use crate::error::Result;
use directories::ProjectDirs;
use gridline_engine::engine::{
    AST, Cell, CellRef, Grid, ValueCache, create_engine_with_functions_and_cache,
};
use rhai::Engine;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

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
    /// The spreadsheet grid (DashMap is internally Arc-based, clones are cheap)
    pub grid: Grid,
    /// Rhai engine for evaluating formulas
    pub engine: Engine,
    /// Current file path
    pub file_path: Option<PathBuf>,
    /// Whether the grid has been modified
    pub modified: bool,
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
    /// Shared value cache for computed cell values (accessible by engine builtins).
    /// Used for both scalar formula results and array formula spill values.
    /// DashMap is internally Arc-based, clones are cheap.
    pub value_cache: ValueCache,
    /// Undo stack
    pub undo_stack: Vec<UndoAction>,
    /// Redo stack
    pub redo_stack: Vec<UndoAction>,
}

impl Core {
    /// Create a new core state
    pub fn new() -> Self {
        let grid: Grid = std::sync::Arc::new(dashmap::DashMap::new());
        let value_cache = ValueCache::default();
        let (engine, _, _) = create_engine_with_functions_and_cache(
            grid.clone(),
            value_cache.clone(),
            None,
        );

        let mut core = Core {
            grid,
            engine,
            file_path: None,
            modified: false,
            functions_files: Vec::new(),
            custom_functions: None,
            custom_ast: None,
            dependents: HashMap::new(),
            spill_sources: HashMap::new(),
            value_cache,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        };

        core.load_default_functions();
        core
    }

    /// Create core and load file if provided
    pub fn with_file(path: Option<PathBuf>, functions_files: Vec<PathBuf>) -> Result<Self> {
        let mut core = Self::new();

        // Load custom functions if specified (ignore errors during init)
        for func_path in &functions_files {
            let _ = core.load_functions(func_path);
        }

        if let Some(ref p) = path {
            if p.exists() {
                core.load_file(p)?;
            } else {
                core.file_path = Some(p.clone());
                core.modified = false;
            }
        }
        Ok(core)
    }

    fn load_default_functions(&mut self) {
        let Some(proj) = ProjectDirs::from("com", "gridline", "gridline") else {
            return;
        };
        let mut path = proj.config_dir().to_path_buf();
        path.push("default.rhai");
        if path.exists() {
            self.load_functions_silent(&path);
        }
    }

    fn load_functions_silent(&mut self, path: &std::path::Path) {
        if let Ok(content) = fs::read_to_string(path) {
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
            // Recreate engine because custom functions changed
            let _ = self.recreate_engine_with_functions();
        }
        // Silently ignore errors for default functions
    }

    /// Recreate the Rhai engine with updated custom functions.
    /// This is expensive and should only be called when custom functions change.
    /// Returns any Rhai compilation error message.
    pub(crate) fn recreate_engine_with_functions(&mut self) -> Option<String> {
        let (engine, ast, error) = create_engine_with_functions_and_cache(
            self.grid.clone(),
            self.value_cache.clone(),
            self.custom_functions.as_deref(),
        );
        self.engine = engine;
        self.custom_ast = ast;
        error
    }

    /// Rebuild the reverse dependency map from the grid.
    /// Call this after cells are added, removed, or their formulas change.
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
