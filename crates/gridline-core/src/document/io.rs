use super::Document;
use crate::error::{GridlineError, Result};
use crate::storage::{parse_csv, parse_grd, write_csv, write_grd};
use gridline_engine::engine::CellType;
use std::path::{Path, PathBuf};

impl Document {
    /// Load custom Rhai functions from a file (appends to existing functions).
    /// Returns the path loaded, or an error.
    pub fn load_functions(&mut self, path: &Path) -> Result<PathBuf> {
        let content = std::fs::read_to_string(path)?;

        // Add to list if not already present
        let path_buf = path.to_path_buf();
        if !self.functions_files.contains(&path_buf) {
            self.functions_files.push(path_buf.clone());
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

        // Recreate engine because custom functions changed
        if let Some(err) = self.recreate_engine_with_functions() {
            return Err(GridlineError::Rhai(err));
        }

        Ok(path_buf)
    }

    /// Reload all custom functions from the loaded files.
    /// Returns the number of files reloaded.
    pub fn reload_functions(&mut self) -> Result<usize> {
        if self.functions_files.is_empty() {
            return Err(GridlineError::NoFunctionsLoaded);
        }

        // Re-read all files
        let paths = self.functions_files.clone();
        self.functions_files.clear();
        self.custom_functions = None;

        let mut count = 0;
        for path in paths {
            self.load_functions(&path)?;
            count += 1;
        }
        Ok(count)
    }

    /// Save to current file path.
    /// Returns the path saved to.
    pub fn save_file(&mut self) -> Result<PathBuf> {
        let Some(path) = &self.file_path else {
            return Err(GridlineError::NoFilePath);
        };

        write_grd(path, &self.grid)?;
        self.modified = false;
        Ok(path.clone())
    }

    /// Load from file
    pub fn load_file(&mut self, path: &Path) -> Result<()> {
        let grid = parse_grd(path)?;
        self.grid = grid;

        // Clear caches since we're loading a new grid
        self.value_cache.clear();
        self.spill_sources.clear();

        // Mark all script cells as dirty so they're re-evaluated with current custom functions
        for mut entry in self.grid.iter_mut() {
            if matches!(entry.contents, CellType::Script(_)) {
                entry.dirty = true;
                entry.cached_value = None;
            }
        }

        // Recreate engine because grid was replaced (builtins need new grid reference)
        let _ = self.recreate_engine_with_functions();

        // Rebuild dependencies
        self.rebuild_dependents();

        // Pre-evaluate all cells in dependency order so computed values are ready
        self.evaluate_all_cells();

        self.file_path = Some(path.to_path_buf());
        self.modified = false;
        self.undo_stack.clear();
        self.redo_stack.clear();
        Ok(())
    }

    /// Import CSV data starting at a row/column.
    /// Returns the number of cells imported.
    pub fn import_csv(&mut self, path: &str, start_row: usize, start_col: usize) -> Result<usize> {
        let cells = parse_csv(Path::new(path), start_row, start_col)?;
        let count = cells.len();
        if count == 0 {
            return Err(GridlineError::EmptyCsv);
        }
        for (cell_ref, cell) in cells {
            self.grid.insert(cell_ref, cell);
        }
        self.modified = true;
        // Clear caches/spills and mark scripts dirty so dependent formulas re-evaluate
        self.value_cache.clear();
        self.spill_sources.clear();
        self.invalidate_script_cache();
        // Rebuild dependencies (DashMap shares data, so builtins already see updates)
        self.rebuild_dependents();
        Ok(count)
    }

    #[cfg(test)]
    pub(crate) fn import_csv_raw(
        &mut self,
        csv_content: &str,
        start_row: usize,
        start_col: usize,
    ) -> Result<usize> {
        let mut count = 0;
        for (row_idx, line) in csv_content.lines().enumerate() {
            for (col_idx, field) in crate::storage::csv::parse_csv_line(line)
                .into_iter()
                .enumerate()
            {
                if field.is_empty() {
                    continue;
                }
                let cell_ref =
                    gridline_engine::engine::CellRef::new(start_row + row_idx, start_col + col_idx);
                let cell = crate::storage::csv::parse_csv_field(&field);
                self.grid.insert(cell_ref, cell);
                count += 1;
            }
        }
        if count == 0 {
            return Err(GridlineError::EmptyCsv);
        }
        self.modified = true;
        self.value_cache.clear();
        self.spill_sources.clear();
        self.invalidate_script_cache();
        // Rebuild dependencies (DashMap shares data, so builtins already see updates)
        self.rebuild_dependents();
        Ok(count)
    }

    /// Export grid to CSV file
    pub fn export_csv(
        &mut self,
        path: &str,
        range: Option<((usize, usize), (usize, usize))>,
    ) -> Result<()> {
        write_csv(Path::new(path), &self.grid, range)?;
        Ok(())
    }
}
