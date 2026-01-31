use super::{Core, UndoAction};
use gridline_engine::engine::{Cell, CellRef, CellType, ShiftOperation, shift_formula_references};

impl Core {
    /// Mark all cells that depend (transitively) on the changed cell as dirty
    fn mark_dependents_dirty(&mut self, changed_cell: &CellRef) {
        let mut to_process = vec![changed_cell.clone()];
        let mut visited = std::collections::HashSet::new();
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

    fn invalidate_script_cache(&mut self) {
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
    pub fn set_cell_from_input(&mut self, cell_ref: CellRef, input: &str) -> crate::error::Result<()> {
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
                self.status_message = "Error: Circular dependency detected".to_string();
                return Ok(());
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
        self.status_message.clear();

        // Clear any spill originating from this cell
        self.clear_spill_from(&cell_ref);

        // Recreate engine with updated grid
        self.recreate_engine();

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

            self.recreate_engine();
            self.mark_dependents_dirty(cell_ref);
        }
    }

    /// Insert a row above the specified row
    pub fn insert_row(&mut self, at_row: usize) {
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
        self.invalidate_script_cache();
        self.recreate_engine();
        self.modified = true;
        self.status_message = format!("Inserted row at {}", at_row + 1);
    }

    /// Delete the specified row
    pub fn delete_row(&mut self, at_row: usize) {
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
        self.invalidate_script_cache();
        self.recreate_engine();
        self.modified = true;
        self.status_message = format!("Deleted row {}", at_row + 1);
    }

    /// Insert a column left of the specified column
    pub fn insert_column(&mut self, at_col: usize) {
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

        // Clear spill/computed maps and rebuild
        self.spill_sources.clear();
        self.spill_map.clear();
        self.computed_map.clear();
        self.invalidate_script_cache();
        self.recreate_engine();
        self.modified = true;
        self.status_message = format!("Inserted column at {}", CellRef::col_to_letters(at_col));
    }

    /// Delete the specified column
    pub fn delete_column(&mut self, at_col: usize) {
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

        // Clear spill/computed maps and rebuild
        self.spill_sources.clear();
        self.spill_map.clear();
        self.computed_map.clear();
        self.invalidate_script_cache();
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
        self.recreate_engine();

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
    use super::Core;
    use gridline_engine::engine::CellRef;

    #[test]
    fn test_delete_column_clears_spill_state() {
        let mut core = Core::new();
        core.set_cell_from_input(CellRef::new(0, 1), "1").unwrap(); // B1
        core.set_cell_from_input(CellRef::new(1, 1), "2").unwrap(); // B2
        core.set_cell_from_input(CellRef::new(2, 1), "3").unwrap(); // B3
        core.set_cell_from_input(CellRef::new(0, 0), "=VEC(B1:B3)").unwrap(); // A1

        let _ = core.get_cell_display(&CellRef::new(0, 0));
        assert!(core.spill_map.contains_key(&CellRef::new(1, 0)));
        assert!(core.spill_sources.contains_key(&CellRef::new(1, 0)));

        core.delete_column(1);
        assert!(core.spill_map.is_empty());
        assert!(core.spill_sources.is_empty());
    }
}
