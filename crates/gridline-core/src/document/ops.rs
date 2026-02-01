use super::{Document, UndoAction};
use crate::error::{GridlineError, Result};
use gridline_engine::engine::{shift_formula_references, Cell, CellRef, CellType, ShiftOperation};

/// Dimension for row/column operations
#[derive(Copy, Clone)]
enum Dimension {
    Row,
    Column,
}

impl Dimension {
    /// Get the coordinate value from a CellRef for this dimension
    fn get_coord(&self, cell_ref: &CellRef) -> usize {
        match self {
            Dimension::Row => cell_ref.row,
            Dimension::Column => cell_ref.col,
        }
    }

    /// Create a new CellRef with modified coordinate in this dimension
    fn new_cell_ref(&self, cell_ref: &CellRef, new_coord: usize) -> CellRef {
        match self {
            Dimension::Row => CellRef::new(new_coord, cell_ref.col),
            Dimension::Column => CellRef::new(cell_ref.row, new_coord),
        }
    }
}

impl Document {
    /// Mark all cells that depend (transitively) on the changed cell as dirty
    fn mark_dependents_dirty(&mut self, changed_cell: &CellRef) {
        let mut to_process = vec![changed_cell.clone()];
        let mut visited = std::collections::HashSet::new();
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
                    // Clear any cached value and spill output for this dependent.
                    self.clear_spill_from(&dep);
                    to_process.push(dep.clone());
                }
            }
        }
    }

    pub(crate) fn invalidate_script_cache(&mut self) {
        for mut entry in self.grid.iter_mut() {
            if let CellType::Script(_) = entry.contents {
                entry.dirty = true;
                entry.cached_value = None;
            }
        }
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
        if self.undo_stack.len() > super::state::MAX_UNDO_STACK {
            self.undo_stack.remove(0);
        }
    }

    /// Set cell contents from input string.
    pub fn set_cell_from_input(&mut self, cell_ref: CellRef, input: &str) -> Result<()> {
        let cell = Cell::from_input(input);

        // Check for circular dependencies if it's a script
        if let CellType::Script(_) = &cell.contents {
            // Temporarily insert to check for cycles
            let old_cell = self.grid.get(&cell_ref).map(|r| r.clone());
            self.grid.insert(cell_ref.clone(), cell.clone());
            if gridline_engine::engine::detect_cycle(&cell_ref, &self.grid).is_some() {
                // Restore old state
                match old_cell {
                    Some(c) => {
                        self.grid.insert(cell_ref, c);
                    }
                    None => {
                        self.grid.remove(&cell_ref);
                    }
                }
                return Err(GridlineError::CircularDependency);
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

        // Clear any spill originating from this cell
        self.clear_spill_from(&cell_ref);

        // Rebuild dependencies (DashMap shares data, so builtins already see updates)
        self.rebuild_dependents();

        // Mark dependent cells as dirty
        self.mark_dependents_dirty(&cell_ref);

        Ok(())
    }

    /// Clear the specified cell
    pub fn clear_cell(&mut self, cell_ref: &CellRef) {
        if self.grid.get(cell_ref).is_some() {
            self.push_undo(cell_ref.clone(), None);
            self.grid.remove(cell_ref);
            self.modified = true;

            // Clear any spill originating from this cell
            self.clear_spill_from(cell_ref);

            // Rebuild dependencies
            self.rebuild_dependents();
            self.mark_dependents_dirty(cell_ref);
        }
    }

    /// Generic insert operation for row or column
    fn insert_dimension(&mut self, dim: Dimension, at: usize) {
        // Collect all cells at coord >= at
        let cells_to_move: Vec<(CellRef, Cell)> = self
            .grid
            .iter()
            .filter(|entry| dim.get_coord(entry.key()) >= at)
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        // Remove them from grid
        for (cell_ref, _) in &cells_to_move {
            self.grid.remove(cell_ref);
        }

        // Update ALL formulas in the grid with shifted references
        let op = match dim {
            Dimension::Row => ShiftOperation::InsertRow(at),
            Dimension::Column => ShiftOperation::InsertColumn(at),
        };
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

        // Reinsert moved cells with coord + 1, also shifting their formulas
        for (cell_ref, cell) in cells_to_move {
            let coord = dim.get_coord(&cell_ref);
            let new_ref = dim.new_cell_ref(&cell_ref, coord + 1);
            let new_cell = if let CellType::Script(formula) = &cell.contents {
                let new_formula = shift_formula_references(formula, op);
                Cell::new_script(&new_formula)
            } else {
                cell
            };
            self.grid.insert(new_ref, new_cell);
        }

        // Clear spill sources and value cache, then rebuild
        self.spill_sources.clear();
        self.value_cache.clear();
        self.invalidate_script_cache();
        // Rebuild dependencies (DashMap shares data, so builtins already see updates)
        self.rebuild_dependents();
        self.modified = true;
    }

    /// Generic delete operation for row or column
    fn delete_dimension(&mut self, dim: Dimension, at: usize) {
        // Collect cells at the deleted coordinate
        let cells_at: Vec<CellRef> = self
            .grid
            .iter()
            .filter(|entry| dim.get_coord(entry.key()) == at)
            .map(|entry| entry.key().clone())
            .collect();

        // Remove cells at the deleted coordinate
        for cell_ref in cells_at {
            self.grid.remove(&cell_ref);
        }

        // Collect cells after the deleted coordinate
        let cells_to_move: Vec<(CellRef, Cell)> = self
            .grid
            .iter()
            .filter(|entry| dim.get_coord(entry.key()) > at)
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        // Remove them from grid
        for (cell_ref, _) in &cells_to_move {
            self.grid.remove(cell_ref);
        }

        // Update ALL formulas with shifted references
        let op = match dim {
            Dimension::Row => ShiftOperation::DeleteRow(at),
            Dimension::Column => ShiftOperation::DeleteColumn(at),
        };
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

        // Reinsert moved cells with coord - 1, also shifting their formulas
        for (cell_ref, cell) in cells_to_move {
            let coord = dim.get_coord(&cell_ref);
            let new_ref = dim.new_cell_ref(&cell_ref, coord - 1);
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

        // Clear spill sources and value cache, then rebuild
        self.spill_sources.clear();
        self.value_cache.clear();
        self.invalidate_script_cache();
        // Rebuild dependencies (DashMap shares data, so builtins already see updates)
        self.rebuild_dependents();
        self.modified = true;
    }

    /// Insert a row above the specified row
    pub fn insert_row(&mut self, at_row: usize) {
        self.insert_dimension(Dimension::Row, at_row);
    }

    /// Delete the specified row
    pub fn delete_row(&mut self, at_row: usize) {
        self.delete_dimension(Dimension::Row, at_row);
    }

    /// Insert a column left of the specified column
    pub fn insert_column(&mut self, at_col: usize) {
        self.insert_dimension(Dimension::Column, at_col);
    }

    /// Delete the specified column
    pub fn delete_column(&mut self, at_col: usize) {
        self.delete_dimension(Dimension::Column, at_col);
    }

    /// Undo the last action
    pub fn undo(&mut self) -> Result<()> {
        let action = self.undo_stack.pop().ok_or(GridlineError::NothingToUndo)?;

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

        // Rebuild dependencies (DashMap shares data, so builtins already see updates)
        self.rebuild_dependents();
        self.mark_dependents_dirty(&cell_ref);
        Ok(())
    }

    /// Redo the last undone action
    pub fn redo(&mut self) -> Result<()> {
        let action = self.redo_stack.pop().ok_or(GridlineError::NothingToRedo)?;

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

        // Rebuild dependencies (DashMap shares data, so builtins already see updates)
        self.rebuild_dependents();
        self.mark_dependents_dirty(&cell_ref);
        Ok(())
    }

    /// Paste cells at a base row/column, recording undo and dependencies.
    pub fn paste_cells(
        &mut self,
        base_row: usize,
        base_col: usize,
        clipboard_cells: &[(usize, usize, Cell)],
    ) -> usize {
        let mut pasted_cells = Vec::new();
        for (rel_row, rel_col, cell) in clipboard_cells {
            let target = CellRef::new(base_row + rel_row, base_col + rel_col);
            self.push_undo(target.clone(), Some(cell.clone()));
            self.grid.insert(target.clone(), cell.clone());
            pasted_cells.push(target);
        }

        if pasted_cells.is_empty() {
            return 0;
        }

        self.modified = true;
        // Rebuild dependencies (DashMap shares data, so builtins already see updates)
        self.rebuild_dependents();

        let count = pasted_cells.len();
        // Mark dependents of all pasted cells as dirty
        for cell_ref in &pasted_cells {
            self.mark_dependents_dirty(cell_ref);
        }

        count
    }
}

#[cfg(test)]
mod tests {
    use super::Document;
    use gridline_engine::engine::CellRef;

    #[test]
    fn test_delete_column_clears_spill_state() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(0, 1), "1").unwrap(); // B1
        core.set_cell_from_input(CellRef::new(1, 1), "2").unwrap(); // B2
        core.set_cell_from_input(CellRef::new(2, 1), "3").unwrap(); // B3
        core.set_cell_from_input(CellRef::new(0, 0), "=VEC(B1:B3)")
            .unwrap(); // A1

        let _ = core.get_cell_display(&CellRef::new(0, 0));
        assert!(core.value_cache.contains_key(&CellRef::new(1, 0)));
        assert!(core.spill_sources.contains_key(&CellRef::new(1, 0)));

        core.delete_column(1);
        assert!(core.value_cache.is_empty());
        assert!(core.spill_sources.is_empty());
    }

    #[test]
    fn test_spill_conflict_clears_stale_spill() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(0, 1), "1").unwrap(); // B1
        core.set_cell_from_input(CellRef::new(1, 1), "2").unwrap(); // B2
        core.set_cell_from_input(CellRef::new(2, 1), "3").unwrap(); // B3
        core.set_cell_from_input(CellRef::new(0, 0), "=VEC(B1:B3)")
            .unwrap(); // A1

        let _ = core.get_cell_display(&CellRef::new(0, 0));
        let spill_cell = CellRef::new(1, 0); // A2
        assert!(core.spill_sources.contains_key(&spill_cell));
        assert!(core.value_cache.contains_key(&spill_cell));

        // Introduce a conflict in the spill range.
        core.set_cell_from_input(spill_cell.clone(), "\"x\"")
            .unwrap();

        // Force A1 to re-evaluate without clearing spill state first.
        if let Some(mut cell) = core.grid.get_mut(&CellRef::new(0, 0)) {
            cell.dirty = true;
            cell.cached_value = None;
        }

        let display = core.get_cell_display(&CellRef::new(0, 0));
        assert_eq!(display, "#SPILL!");
        assert!(!core.spill_sources.contains_key(&spill_cell));
        assert!(!core.value_cache.contains_key(&spill_cell));
    }
}
