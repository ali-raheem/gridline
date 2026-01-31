use super::Core;
use crate::error::Result;
use crate::storage::{parse_csv, parse_grd, write_csv, write_grd};

impl Core {
    /// Load custom Rhai functions from a file (appends to existing functions)
    pub fn load_functions(&mut self, path: &std::path::Path) {
        match std::fs::read_to_string(path) {
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
        self.grid = std::sync::Arc::new(grid);
        self.recreate_engine();
        self.file_path = Some(path.to_path_buf());
        self.modified = false;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.status_message = format!("Loaded {}", path.display());
        Ok(())
    }

    /// Import CSV data starting at a row/column
    pub fn import_csv(&mut self, path: &str, start_row: usize, start_col: usize) {
        match parse_csv(std::path::Path::new(path), start_row, start_col) {
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
    pub fn export_csv(
        &mut self,
        path: &str,
        range: Option<((usize, usize), (usize, usize))>,
    ) {
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
