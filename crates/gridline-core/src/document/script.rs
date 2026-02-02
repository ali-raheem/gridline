//! Script execution for Rhai scripts that can modify the spreadsheet.
//!
//! This module provides the ability to run Rhai scripts from TUI command mode
//! that have access to cursor position, selection, and write builtins.

use super::{Document, UndoAction};
use crate::error::{GridlineError, Result};
use gridline_engine::builtins::ScriptModifications;
use gridline_engine::engine::{create_script_engine_with_functions, eval_with_functions_script};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Context information injected into scripts before execution.
#[derive(Debug, Clone)]
pub struct ScriptContext {
    /// Current cursor row (0-indexed)
    pub cursor_row: usize,
    /// Current cursor column (0-indexed)
    pub cursor_col: usize,
    /// Whether visual selection is active
    pub has_selection: bool,
    /// Selection bounds (if has_selection is true)
    /// (top_row, left_col, bottom_row, right_col) - all 0-indexed
    pub selection: Option<(usize, usize, usize, usize)>,
}

impl ScriptContext {
    /// Create a new script context with no selection.
    pub fn new(cursor_row: usize, cursor_col: usize) -> Self {
        ScriptContext {
            cursor_row,
            cursor_col,
            has_selection: false,
            selection: None,
        }
    }

    /// Create a new script context with selection.
    pub fn with_selection(
        cursor_row: usize,
        cursor_col: usize,
        sel_r1: usize,
        sel_c1: usize,
        sel_r2: usize,
        sel_c2: usize,
    ) -> Self {
        ScriptContext {
            cursor_row,
            cursor_col,
            has_selection: true,
            selection: Some((sel_r1, sel_c1, sel_r2, sel_c2)),
        }
    }

    /// Generate the Rhai variable declarations to inject before the script.
    fn to_rhai_declarations(&self) -> String {
        let mut decls = format!(
            "let CURSOR_ROW = {}; let CURSOR_COL = {};\n",
            self.cursor_row, self.cursor_col
        );
        decls.push_str(&format!("let HAS_SELECTION = {};\n", self.has_selection));

        if let Some((r1, c1, r2, c2)) = self.selection {
            decls.push_str(&format!(
                "let SEL_R1 = {}; let SEL_C1 = {}; let SEL_R2 = {}; let SEL_C2 = {};\n",
                r1, c1, r2, c2
            ));
        } else {
            // Provide default values even when no selection (prevents undefined variable errors)
            decls.push_str("let SEL_R1 = 0; let SEL_C1 = 0; let SEL_R2 = 0; let SEL_C2 = 0;\n");
        }

        decls
    }
}

/// Result of script execution.
#[derive(Debug)]
pub struct ScriptResult {
    /// Number of cells modified by the script
    pub cells_modified: usize,
    /// The return value of the script (if any), as a string for display
    pub return_value: Option<String>,
}

impl Document {
    /// Execute a Rhai script with write access to the spreadsheet.
    ///
    /// The script can use:
    /// - All read builtins (cell, value, sum_range, etc.)
    /// - Write builtins (SET_CELL, clear_cell, set_range, clear_range)
    /// - Context variables (CURSOR_ROW, CURSOR_COL, HAS_SELECTION, SEL_R1, etc.)
    ///
    /// All modifications are collected into a single batch undo entry.
    pub fn execute_script(&mut self, script: &str, context: &ScriptContext) -> Result<ScriptResult> {
        // Create modifications tracker
        let modifications: ScriptModifications = Arc::new(Mutex::new(HashMap::new()));

        // Create script engine with write builtins
        let (engine, custom_ast, compile_error) = create_script_engine_with_functions(
            self.grid.clone(),
            self.value_cache.clone(),
            modifications.clone(),
            self.custom_functions.as_deref(),
        );

        if let Some(err) = compile_error {
            return Err(GridlineError::Rhai(err));
        }

        // Build the full script with context declarations
        let context_decls = context.to_rhai_declarations();
        let full_script = format!("{}{}", context_decls, script);

        // Execute the script
        let result = eval_with_functions_script(&engine, &full_script, custom_ast.as_ref().map(|_| self.custom_functions.as_deref()).flatten());

        let return_value = match &result {
            Ok(val) if !val.is_unit() => Some(val.to_string()),
            _ => None,
        };

        // Check for execution errors
        if let Err(e) = result {
            return Err(GridlineError::Rhai(e));
        }

        // Collect modifications and create undo batch
        let mods = modifications.lock().unwrap();
        let cells_modified = mods.len();

        if cells_modified > 0 {
            // Build undo actions from modifications
            let undo_actions: Vec<UndoAction> = mods
                .iter()
                .map(|(cell_ref, (old_cell, new_cell))| UndoAction {
                    cell_ref: cell_ref.clone(),
                    old_cell: old_cell.clone(),
                    new_cell: new_cell.clone(),
                })
                .collect();

            // Push batch undo entry
            self.push_undo_batch(undo_actions);

            // Mark document as modified
            self.modified = true;

            // Rebuild dependencies once for all changes
            self.rebuild_dependents();

            // Mark dependents dirty
            for cell_ref in mods.keys() {
                self.mark_dependents_dirty_public(cell_ref);
            }
        }

        Ok(ScriptResult {
            cells_modified,
            return_value,
        })
    }

    /// Helper to mark dependents dirty (called from script module)
    fn mark_dependents_dirty_public(&mut self, changed_cell: &gridline_engine::engine::CellRef) {
        // Use the same logic as mark_dependents_dirty in ops.rs
        let mut to_process = vec![changed_cell.clone()];
        let mut visited = std::collections::HashSet::new();
        while let Some(cell_ref) = to_process.pop() {
            if !visited.insert(cell_ref.clone()) {
                continue;
            }

            if let Some(deps) = self.dependents.get(&cell_ref) {
                for dep in deps.clone() {
                    if let Some(mut cell) = self.grid.get_mut(&dep) {
                        cell.dirty = true;
                        cell.cached_value = None;
                    }
                    self.clear_spill_from(&dep);
                    to_process.push(dep.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridline_engine::engine::CellRef;

    #[test]
    fn test_script_context_declarations() {
        let ctx = ScriptContext::new(5, 3);
        let decls = ctx.to_rhai_declarations();
        assert!(decls.contains("let CURSOR_ROW = 5"));
        assert!(decls.contains("let CURSOR_COL = 3"));
        assert!(decls.contains("let HAS_SELECTION = false"));
    }

    #[test]
    fn test_script_context_with_selection() {
        let ctx = ScriptContext::with_selection(5, 3, 2, 1, 10, 5);
        let decls = ctx.to_rhai_declarations();
        assert!(decls.contains("let HAS_SELECTION = true"));
        assert!(decls.contains("let SEL_R1 = 2"));
        assert!(decls.contains("let SEL_C1 = 1"));
        assert!(decls.contains("let SEL_R2 = 10"));
        assert!(decls.contains("let SEL_C2 = 5"));
    }

    #[test]
    fn test_execute_script_SET_CELL() {
        let mut doc = Document::new();
        let ctx = ScriptContext::new(0, 0);

        let result = doc.execute_script("SET_CELL(0, 0, 42)", &ctx).unwrap();
        assert_eq!(result.cells_modified, 1);

        // Verify cell was set
        let cell = doc.grid.get(&CellRef::new(0, 0)).unwrap();
        assert!(matches!(
            cell.contents,
            gridline_engine::engine::CellType::Number(n) if (n - 42.0).abs() < 0.001
        ));
    }

    #[test]
    fn test_execute_script_SET_CELL_a1_notation() {
        let mut doc = Document::new();
        let ctx = ScriptContext::new(0, 0);

        let result = doc.execute_script(r#"SET_CELL("B2", "hello")"#, &ctx).unwrap();
        assert_eq!(result.cells_modified, 1);

        // Verify cell was set at B2 (row 1, col 1)
        let cell = doc.grid.get(&CellRef::new(1, 1)).unwrap();
        assert!(matches!(
            &cell.contents,
            gridline_engine::engine::CellType::Text(s) if s == "hello"
        ));
    }

    #[test]
    fn test_execute_script_batch_undo() {
        let mut doc = Document::new();
        let ctx = ScriptContext::new(0, 0);

        // Set multiple cells
        doc.execute_script(
            r#"
            SET_CELL(0, 0, 1);
            SET_CELL(0, 1, 2);
            SET_CELL(0, 2, 3);
            "#,
            &ctx,
        )
        .unwrap();

        // Verify all cells were set
        assert!(doc.grid.get(&CellRef::new(0, 0)).is_some());
        assert!(doc.grid.get(&CellRef::new(0, 1)).is_some());
        assert!(doc.grid.get(&CellRef::new(0, 2)).is_some());

        // Undo should revert all three
        doc.undo().unwrap();
        assert!(doc.grid.get(&CellRef::new(0, 0)).is_none());
        assert!(doc.grid.get(&CellRef::new(0, 1)).is_none());
        assert!(doc.grid.get(&CellRef::new(0, 2)).is_none());
    }

    #[test]
    fn test_execute_script_with_context() {
        let mut doc = Document::new();
        let ctx = ScriptContext::with_selection(0, 0, 0, 0, 2, 0);

        // Fill selection using context variables
        doc.execute_script(
            r#"
            for r in SEL_R1..=SEL_R2 {
                for c in SEL_C1..=SEL_C2 {
                    SET_CELL(r, c, r + 1);
                }
            }
            "#,
            &ctx,
        )
        .unwrap();

        // Verify cells
        let cell0 = doc.grid.get(&CellRef::new(0, 0)).unwrap();
        let cell1 = doc.grid.get(&CellRef::new(1, 0)).unwrap();
        let cell2 = doc.grid.get(&CellRef::new(2, 0)).unwrap();
        assert!(matches!(cell0.contents, gridline_engine::engine::CellType::Number(n) if (n - 1.0).abs() < 0.001));
        assert!(matches!(cell1.contents, gridline_engine::engine::CellType::Number(n) if (n - 2.0).abs() < 0.001));
        assert!(matches!(cell2.contents, gridline_engine::engine::CellType::Number(n) if (n - 3.0).abs() < 0.001));
    }
}
