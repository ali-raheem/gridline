use super::Document;
use crate::error::{GridlineError, Result};
use crate::storage::{parse_csv, parse_grd, write_csv, write_grd};
use gridline_engine::engine::create_engine_with_functions_and_cache;
use gridline_engine::engine::CellType;
use std::path::{Path, PathBuf};

impl Document {
    /// Load custom Rhai functions from a file (appends to existing functions).
    /// Returns the path loaded, or an error.
    pub fn load_functions(&mut self, path: &Path) -> Result<PathBuf> {
        let content = std::fs::read_to_string(path)?;

        let path_buf = path.to_path_buf();
        let mut new_functions_files = self.functions_files.clone();
        if !new_functions_files.contains(&path_buf) {
            new_functions_files.push(path_buf.clone());
        }

        let new_custom_functions = if let Some(existing) = &self.custom_functions {
            format!("{}\n\n{}", existing, content)
        } else {
            content
        };

        // Compile in a temporary engine first so failures don't mutate state.
        let (engine, custom_ast, compile_error) = create_engine_with_functions_and_cache(
            self.grid.clone(),
            self.value_cache.clone(),
            Some(&new_custom_functions),
        );
        if let Some(err) = compile_error {
            return Err(GridlineError::RhaiCompile(err));
        }

        // Commit only after successful compilation.
        self.functions_files = new_functions_files;
        self.custom_functions = Some(new_custom_functions);
        self.engine = engine;
        self.custom_ast = custom_ast;

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

    /// Import CSV data starting at a column/row.
    /// Returns the number of cells imported.
    pub fn import_csv(&mut self, path: &str, start_col: usize, start_row: usize) -> Result<usize> {
        let cells = parse_csv(Path::new(path), start_col, start_row)?;
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
        start_col: usize,
        start_row: usize,
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
                    gridline_engine::engine::CellRef::new(start_col + col_idx, start_row + row_idx);
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
        write_csv(Path::new(path), self, range)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Document;
    use gridline_engine::engine::CellRef;
    use std::io::Write;

    #[test]
    fn test_load_functions_failure_is_transactional() {
        let mut doc = Document::new();

        let good_path = std::env::temp_dir().join(format!(
            "gridline_good_funcs_{}_{}_{:?}.rhai",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::thread::current().id(),
        ));
        let bad_path = std::env::temp_dir().join(format!(
            "gridline_bad_funcs_{}_{}_{:?}.rhai",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::thread::current().id(),
        ));

        struct Cleanup(std::path::PathBuf, std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
                let _ = std::fs::remove_file(&self.1);
            }
        }
        let _cleanup = Cleanup(good_path.clone(), bad_path.clone());

        {
            let mut f = std::fs::File::create(&good_path).unwrap();
            writeln!(f, "fn double(x) {{ x * 2 }}").unwrap();
        }
        {
            let mut f = std::fs::File::create(&bad_path).unwrap();
            writeln!(f, "fn broken(x) {{ x + }}").unwrap();
        }

        doc.load_functions(&good_path).unwrap();
        let files_before = doc.functions_files.clone();
        let custom_before = doc.custom_functions.clone();

        let err = doc.load_functions(&bad_path);
        assert!(err.is_err());
        assert_eq!(doc.functions_files, files_before);
        assert_eq!(doc.custom_functions, custom_before);

        doc.set_cell_from_input(CellRef::new(0, 0), "=double(3)").unwrap();
        assert_eq!(doc.get_cell_display(&CellRef::new(0, 0)), "6");
    }

    #[test]
    fn test_with_file_reports_function_load_errors() {
        let bad_path = std::env::temp_dir().join(format!(
            "gridline_with_file_bad_funcs_{}_{}_{:?}.rhai",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::thread::current().id(),
        ));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(bad_path.clone());

        let mut f = std::fs::File::create(&bad_path).unwrap();
        writeln!(f, "fn broken(x) {{ x + }}").unwrap();

        let result = Document::with_file(None, vec![bad_path]);
        assert!(result.is_err());
    }
}
