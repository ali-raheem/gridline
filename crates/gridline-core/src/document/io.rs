use super::Document;
use crate::error::{GridlineError, Result};
use crate::storage::{parse_csv, parse_grd, write_csv, write_grd};
use gridline_engine::engine::create_engine_with_functions_and_cache;
use gridline_engine::engine::CellType;
use std::path::{Path, PathBuf};

const MAX_FUNCTION_FILE_BYTES: u64 = 1_048_576; // 1 MiB

fn read_functions_file(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > MAX_FUNCTION_FILE_BYTES {
        return Err(GridlineError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Refusing to read {}: functions file too large ({} bytes, max {})",
                path.display(),
                meta.len(),
                MAX_FUNCTION_FILE_BYTES
            ),
        )));
    }
    Ok(std::fs::read_to_string(path)?)
}

impl Document {
    /// Load custom Rhai functions from a file (appends to existing functions).
    /// Returns the path loaded, or an error.
    pub fn load_functions(&mut self, path: &Path) -> Result<PathBuf> {
        let path_buf = std::fs::canonicalize(path)?;
        let content = read_functions_file(&path_buf)?;

        if self.functions_files.contains(&path_buf) {
            // Already loaded: keep current compiled state unchanged.
            return Ok(path_buf.clone());
        }

        let mut new_functions_files = self.functions_files.clone();
        new_functions_files.push(path_buf.clone());

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

        let paths = self.functions_files.clone();
        let mut merged = String::new();
        for (idx, path) in paths.iter().enumerate() {
            let content = read_functions_file(path)?;
            if idx > 0 {
                merged.push_str("\n\n");
            }
            merged.push_str(&content);
        }

        let (engine, custom_ast, compile_error) = create_engine_with_functions_and_cache(
            self.grid.clone(),
            self.value_cache.clone(),
            Some(&merged),
        );
        if let Some(err) = compile_error {
            return Err(GridlineError::RhaiCompile(err));
        }

        self.custom_functions = Some(merged);
        self.engine = engine;
        self.custom_ast = custom_ast;

        Ok(paths.len())
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

        // Build engine for the new grid first so load is transactional.
        let (engine, custom_ast, compile_error) = create_engine_with_functions_and_cache(
            grid.clone(),
            self.value_cache.clone(),
            self.custom_functions.as_deref(),
        );
        if let Some(err) = compile_error {
            return Err(GridlineError::RhaiCompile(err));
        }

        self.grid = grid;
        self.engine = engine;
        self.custom_ast = custom_ast;

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
    use super::{Document, MAX_FUNCTION_FILE_BYTES};
    use crate::error::GridlineError;
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

        doc.set_cell_from_input(CellRef::new(0, 0), "=double(3)")
            .unwrap();
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

    #[test]
    fn test_load_functions_rejects_oversized_file() {
        let mut doc = Document::new();

        let path = std::env::temp_dir().join(format!(
            "gridline_oversized_funcs_{}_{}_{:?}.rhai",
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
        let _cleanup = Cleanup(path.clone());

        let oversized = "a".repeat(MAX_FUNCTION_FILE_BYTES as usize + 1);
        std::fs::write(&path, oversized).unwrap();

        let err = doc.load_functions(&path).unwrap_err();
        match err {
            GridlineError::Io(io_err) => {
                assert!(io_err.to_string().contains("too large"));
            }
            other => panic!("expected io error for oversized file, got {other:?}"),
        }
    }

    #[test]
    fn test_reload_functions_failure_is_transactional() {
        let mut doc = Document::new();

        let path = std::env::temp_dir().join(format!(
            "gridline_reload_funcs_{}_{}_{:?}.rhai",
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
        let _cleanup = Cleanup(path.clone());

        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "fn triple(x) {{ x * 3 }}").unwrap();
        }
        doc.load_functions(&path).unwrap();
        let before = doc.custom_functions.clone();

        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "fn broken(x) {{ x + }}").unwrap();
        }

        let err = doc.reload_functions();
        assert!(err.is_err());
        assert_eq!(doc.custom_functions, before);

        doc.set_cell_from_input(CellRef::new(0, 0), "=triple(3)")
            .unwrap();
        assert_eq!(doc.get_cell_display(&CellRef::new(0, 0)), "9");
    }

    #[test]
    fn test_load_functions_same_path_is_idempotent() {
        let mut doc = Document::new();

        let path = std::env::temp_dir().join(format!(
            "gridline_idempotent_funcs_{}_{}_{:?}.rhai",
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
        let _cleanup = Cleanup(path.clone());

        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "fn double(x) {{ x * 2 }}").unwrap();
        }

        doc.load_functions(&path).unwrap();
        let before = doc.custom_functions.clone();

        // Loading the same file again should be a no-op, not a duplicate append.
        doc.load_functions(&path).unwrap();
        assert_eq!(doc.functions_files.len(), 1);
        assert_eq!(doc.custom_functions, before);

        doc.set_cell_from_input(CellRef::new(0, 0), "=double(4)")
            .unwrap();
        assert_eq!(doc.get_cell_display(&CellRef::new(0, 0)), "8");
    }

    #[test]
    fn test_load_functions_equivalent_paths_are_idempotent() {
        let mut doc = Document::new();

        let path = std::env::temp_dir().join(format!(
            "gridline_idempotent_funcs_equiv_{}_{}_{:?}.rhai",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::thread::current().id(),
        ));
        let equiv_path = path
            .parent()
            .unwrap()
            .join(".")
            .join(path.file_name().unwrap());
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());

        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "fn double(x) {{ x * 2 }}").unwrap();
        }

        let first = doc.load_functions(&path).unwrap();
        let second = doc.load_functions(&equiv_path).unwrap();
        assert_eq!(first, second);
        assert_eq!(doc.functions_files.len(), 1);

        doc.set_cell_from_input(CellRef::new(0, 0), "=double(5)")
            .unwrap();
        assert_eq!(doc.get_cell_display(&CellRef::new(0, 0)), "10");
    }

    #[test]
    fn test_load_file_fails_transactionally_on_custom_function_compile_error() {
        let mut doc = Document::new();
        doc.set_cell_from_input(CellRef::new(2, 2), "42").unwrap(); // C3 existing state

        let grd_path = std::env::temp_dir().join(format!(
            "gridline_load_file_txn_{}_{}_{:?}.grd",
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
        let _cleanup = Cleanup(grd_path.clone());
        std::fs::write(&grd_path, "A1: 1\n").unwrap();

        // Simulate corrupted in-memory custom function state.
        doc.custom_functions = Some("fn broken(x) { x + }".to_string());
        let old_file_path = doc.file_path.clone();

        let result = doc.load_file(&grd_path);
        assert!(result.is_err());
        assert!(matches!(result, Err(GridlineError::RhaiCompile(_))));

        // Ensure existing document state was not replaced.
        assert!(doc.grid.contains_key(&CellRef::new(2, 2))); // C3 still present
        assert!(!doc.grid.contains_key(&CellRef::new(0, 0))); // A1 from file not committed
        assert_eq!(doc.file_path, old_file_path);
    }
}
