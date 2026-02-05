use super::{Document, UndoAction, UndoEntry};
use crate::error::{GridlineError, Result};
use gridline_engine::engine::{
    Cell, CellRef, CellType, ShiftOperation, offset_formula_references, shift_formula_references,
};

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
            Dimension::Row => CellRef::new(cell_ref.col, new_coord),
            Dimension::Column => CellRef::new(new_coord, cell_ref.row),
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

    /// Prepare a cell position for overwrite by clearing stale spill/cache state.
    /// Returns any spill source that was invalidated so dependents can be dirtied.
    pub(crate) fn prepare_overwrite(&mut self, cell_ref: &CellRef) -> Option<CellRef> {
        // If writing into a spill output, invalidate the source spill and force source re-eval.
        let spilled_from = self.spill_sources.get(cell_ref).cloned();
        if let Some(source) = &spilled_from {
            self.clear_spill_from(source);
            if let Some(mut src_cell) = self.grid.get_mut(source) {
                src_cell.dirty = true;
                src_cell.cached_value = None;
            }
        }

        // If this position is itself a spill source, clear its old spill output.
        self.clear_spill_from(cell_ref);

        // Also remove direct stale entries at this exact position.
        self.spill_sources.remove(cell_ref);
        self.value_cache.remove(cell_ref);

        spilled_from
    }

    /// Apply a historical cell state (undo/redo) with the same overwrite semantics as edits.
    fn apply_history_cell_state(
        &mut self,
        cell_ref: &CellRef,
        state: Option<Cell>,
        additionally_dirty: &mut Vec<CellRef>,
    ) {
        if let Some(source) = self.prepare_overwrite(cell_ref) {
            additionally_dirty.push(source);
        }
        match state {
            Some(mut cell) => {
                // Historical script snapshots may contain stale cache/spill state.
                if matches!(cell.contents, CellType::Script(_)) {
                    cell.dirty = true;
                    cell.cached_value = None;
                }
                self.grid.insert(cell_ref.clone(), cell);
            }
            None => {
                self.grid.remove(cell_ref);
            }
        }
    }

    /// Push an undo action before modifying a cell
    fn push_undo(&mut self, cell_ref: CellRef, new_cell: Option<Cell>) {
        let old_cell = self.grid.get(&cell_ref).map(|r| r.clone());
        self.undo_stack.push(UndoEntry::Single(UndoAction {
            cell_ref,
            old_cell,
            new_cell,
        }));
        self.redo_stack.clear();
        if self.undo_stack.len() > super::state::MAX_UNDO_STACK {
            self.undo_stack.remove(0);
        }
    }

    /// Push a batch of undo actions (e.g., from script execution)
    pub fn push_undo_batch(&mut self, actions: Vec<UndoAction>) {
        if actions.is_empty() {
            return;
        }
        self.undo_stack.push(UndoEntry::Batch(actions));
        self.redo_stack.clear();
        if self.undo_stack.len() > super::state::MAX_UNDO_STACK {
            self.undo_stack.remove(0);
        }
    }

    /// Set cell contents from input string.
    pub fn set_cell_from_input(&mut self, cell_ref: CellRef, input: &str) -> Result<()> {
        let cell = Cell::from_input(input);
        let mut invalidated_spill_sources = Vec::new();

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
            if let Some(source) = self.prepare_overwrite(&cell_ref) {
                invalidated_spill_sources.push(source);
            }
            self.push_undo(cell_ref.clone(), Some(cell.clone()));
            self.grid.insert(cell_ref.clone(), cell);
        } else {
            if let Some(source) = self.prepare_overwrite(&cell_ref) {
                invalidated_spill_sources.push(source);
            }
            self.push_undo(cell_ref.clone(), Some(cell.clone()));
            self.grid.insert(cell_ref.clone(), cell);
        }

        self.modified = true;

        // Rebuild dependencies (DashMap shares data, so builtins already see updates)
        self.rebuild_dependents();

        // Mark dependent cells as dirty
        self.mark_dependents_dirty(&cell_ref);
        for source in invalidated_spill_sources {
            if source != cell_ref {
                self.mark_dependents_dirty(&source);
            }
        }

        Ok(())
    }

    /// Clear the specified cell
    pub fn clear_cell(&mut self, cell_ref: &CellRef) {
        if self.grid.get(cell_ref).is_some() {
            let invalidated_spill_source = self.prepare_overwrite(cell_ref);
            self.push_undo(cell_ref.clone(), None);
            self.grid.remove(cell_ref);
            self.modified = true;

            // Rebuild dependencies
            self.rebuild_dependents();
            self.mark_dependents_dirty(cell_ref);
            if let Some(source) = invalidated_spill_source
                && &source != cell_ref
            {
                self.mark_dependents_dirty(&source);
            }
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
        let entry = self.undo_stack.pop().ok_or(GridlineError::NothingToUndo)?;

        match entry {
            UndoEntry::Single(action) => {
                // Push inverse to redo stack
                let current = self.grid.get(&action.cell_ref).map(|r| r.clone());
                self.redo_stack.push(UndoEntry::Single(UndoAction {
                    cell_ref: action.cell_ref.clone(),
                    old_cell: action.old_cell.clone(), // State after undo (for undo-after-redo)
                    new_cell: current,                 // State before undo (what redo restores)
                }));

                let cell_ref = action.cell_ref.clone();
                let mut additionally_dirty = Vec::new();

                // Restore old state
                self.apply_history_cell_state(&cell_ref, action.old_cell, &mut additionally_dirty);

                // Rebuild dependencies (DashMap shares data, so builtins already see updates)
                self.rebuild_dependents();
                self.mark_dependents_dirty(&cell_ref);
                for spill_source in additionally_dirty {
                    self.mark_dependents_dirty(&spill_source);
                }
            }
            UndoEntry::Batch(actions) => {
                // Build inverse batch for redo
                let mut redo_actions = Vec::with_capacity(actions.len());
                let mut affected_cells = Vec::with_capacity(actions.len());

                for action in &actions {
                    let current = self.grid.get(&action.cell_ref).map(|r| r.clone());
                    redo_actions.push(UndoAction {
                        cell_ref: action.cell_ref.clone(),
                        old_cell: action.old_cell.clone(), // State after undo
                        new_cell: current,                 // State before undo
                    });
                    affected_cells.push(action.cell_ref.clone());
                }

                self.redo_stack.push(UndoEntry::Batch(redo_actions));

                // Restore old states
                let mut additionally_dirty = Vec::new();
                for action in actions {
                    self.apply_history_cell_state(
                        &action.cell_ref,
                        action.old_cell,
                        &mut additionally_dirty,
                    );
                }

                // Rebuild dependencies once
                self.rebuild_dependents();
                for cell_ref in affected_cells {
                    self.mark_dependents_dirty(&cell_ref);
                }
                for spill_source in additionally_dirty {
                    self.mark_dependents_dirty(&spill_source);
                }
            }
        }
        self.modified = true;
        Ok(())
    }

    /// Redo the last undone action
    pub fn redo(&mut self) -> Result<()> {
        let entry = self.redo_stack.pop().ok_or(GridlineError::NothingToRedo)?;

        match entry {
            UndoEntry::Single(action) => {
                // Push inverse to undo stack
                let current = self.grid.get(&action.cell_ref).map(|r| r.clone());
                self.undo_stack.push(UndoEntry::Single(UndoAction {
                    cell_ref: action.cell_ref.clone(),
                    old_cell: current,
                    new_cell: action.new_cell.clone(),
                }));

                let cell_ref = action.cell_ref.clone();
                let mut additionally_dirty = Vec::new();

                // Apply new state
                self.apply_history_cell_state(&cell_ref, action.new_cell, &mut additionally_dirty);

                // Rebuild dependencies (DashMap shares data, so builtins already see updates)
                self.rebuild_dependents();
                self.mark_dependents_dirty(&cell_ref);
                for spill_source in additionally_dirty {
                    self.mark_dependents_dirty(&spill_source);
                }
            }
            UndoEntry::Batch(actions) => {
                // Build inverse batch for undo
                let mut undo_actions = Vec::with_capacity(actions.len());
                let mut affected_cells = Vec::with_capacity(actions.len());

                for action in &actions {
                    let current = self.grid.get(&action.cell_ref).map(|r| r.clone());
                    undo_actions.push(UndoAction {
                        cell_ref: action.cell_ref.clone(),
                        old_cell: current,
                        new_cell: action.new_cell.clone(),
                    });
                    affected_cells.push(action.cell_ref.clone());
                }

                self.undo_stack.push(UndoEntry::Batch(undo_actions));

                // Apply new states
                let mut additionally_dirty = Vec::new();
                for action in actions {
                    self.apply_history_cell_state(
                        &action.cell_ref,
                        action.new_cell,
                        &mut additionally_dirty,
                    );
                }

                // Rebuild dependencies once
                self.rebuild_dependents();
                for cell_ref in affected_cells {
                    self.mark_dependents_dirty(&cell_ref);
                }
                for spill_source in additionally_dirty {
                    self.mark_dependents_dirty(&spill_source);
                }
            }
        }
        self.modified = true;
        Ok(())
    }

    /// Paste cells at a base column/row, recording undo and dependencies.
    pub fn paste_cells(
        &mut self,
        base_col: usize,
        base_row: usize,
        source_base_col: usize,
        source_base_row: usize,
        clipboard_cells: &[(usize, usize, Cell)],
    ) -> Result<usize> {
        let delta_col = base_col as isize - source_base_col as isize;
        let delta_row = base_row as isize - source_base_row as isize;
        let mut prepared = Vec::new();

        for (rel_col, rel_row, cell) in clipboard_cells {
            let target = CellRef::new(base_col + rel_col, base_row + rel_row);

            let pasted_cell = match &cell.contents {
                CellType::Script(formula) => {
                    let shifted = offset_formula_references(formula, delta_col, delta_row);
                    Cell::new_script(&shifted)
                }
                _ => cell.clone(),
            };
            prepared.push((target, pasted_cell));
        }

        if prepared.is_empty() {
            return Ok(0);
        }

        // Validate cycle safety transactionally before mutating spill/cache/undo state.
        let mut old_cells: std::collections::HashMap<CellRef, Option<Cell>> =
            std::collections::HashMap::new();
        for (target, _) in &prepared {
            old_cells
                .entry(target.clone())
                .or_insert_with(|| self.grid.get(target).map(|r| r.clone()));
        }
        for (target, pasted_cell) in &prepared {
            self.grid.insert(target.clone(), pasted_cell.clone());
        }

        let has_cycle = prepared.iter().any(|(target, pasted_cell)| {
            matches!(pasted_cell.contents, CellType::Script(_))
                && gridline_engine::engine::detect_cycle(target, &self.grid).is_some()
        });

        // Restore original grid state after validation pass.
        for (target, old_cell) in old_cells {
            match old_cell {
                Some(cell) => {
                    self.grid.insert(target, cell);
                }
                None => {
                    self.grid.remove(&target);
                }
            }
        }

        if has_cycle {
            return Err(GridlineError::CircularDependency);
        }

        let mut pasted_cells = Vec::new();
        let mut additionally_dirty = Vec::new();
        for (target, pasted_cell) in prepared {
            if let Some(spill_source) = self.prepare_overwrite(&target) {
                additionally_dirty.push(spill_source);
            }
            self.push_undo(target.clone(), Some(pasted_cell.clone()));
            self.grid.insert(target.clone(), pasted_cell);
            pasted_cells.push(target);
        }

        self.modified = true;
        // Rebuild dependencies (DashMap shares data, so builtins already see updates)
        self.rebuild_dependents();

        let count = pasted_cells.len();
        // Mark dependents of all pasted cells as dirty
        for cell_ref in &pasted_cells {
            self.mark_dependents_dirty(cell_ref);
        }
        for spill_source in additionally_dirty {
            self.mark_dependents_dirty(&spill_source);
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::Document;
    use crate::error::GridlineError;
    use gridline_engine::engine::{Cell, CellRef, CellType};

    #[test]
    fn test_delete_column_clears_spill_state() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(1, 0), "1").unwrap(); // B1
        core.set_cell_from_input(CellRef::new(1, 1), "2").unwrap(); // B2
        core.set_cell_from_input(CellRef::new(1, 2), "3").unwrap(); // B3
        core.set_cell_from_input(CellRef::new(0, 0), "=VEC(B1:B3)")
            .unwrap(); // A1

        let _ = core.get_cell_display(&CellRef::new(0, 0));
        assert!(core.value_cache.contains_key(&CellRef::new(0, 1)));
        assert!(core.spill_sources.contains_key(&CellRef::new(0, 1)));

        core.delete_column(1);
        assert!(core.value_cache.is_empty());
        assert!(core.spill_sources.is_empty());
    }

    #[test]
    fn test_spill_conflict_clears_stale_spill() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(1, 0), "1").unwrap(); // B1
        core.set_cell_from_input(CellRef::new(1, 1), "2").unwrap(); // B2
        core.set_cell_from_input(CellRef::new(1, 2), "3").unwrap(); // B3
        core.set_cell_from_input(CellRef::new(0, 0), "=VEC(B1:B3)")
            .unwrap(); // A1

        let _ = core.get_cell_display(&CellRef::new(0, 0));
        let spill_cell = CellRef::new(0, 1); // A2
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

    #[test]
    fn test_paste_over_spill_source_clears_spill_and_invalidates_dependents() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(0, 0), "=SPILL(1..=3)")
            .unwrap(); // A1 source
        core.set_cell_from_input(CellRef::new(2, 0), "=A1").unwrap(); // C1 depends on A1
        core.set_cell_from_input(CellRef::new(1, 0), "99").unwrap(); // B1 source for paste

        // Populate spill/cache state.
        let _ = core.get_cell_display(&CellRef::new(0, 0));
        assert!(core.spill_sources.contains_key(&CellRef::new(0, 1)));
        assert!(core.value_cache.contains_key(&CellRef::new(0, 1)));

        // Paste B1 onto A1.
        let cell = core.grid.get(&CellRef::new(1, 0)).unwrap().clone();
        let pasted = core.paste_cells(0, 0, 1, 0, &[(0, 0, cell)]).unwrap();
        assert_eq!(pasted, 1);

        // Spill state must be removed and dependents must observe new value.
        assert!(!core.spill_sources.contains_key(&CellRef::new(0, 1)));
        assert!(!core.value_cache.contains_key(&CellRef::new(0, 1)));
        assert_eq!(core.get_cell_display(&CellRef::new(0, 0)), "99");
        assert_eq!(core.get_cell_display(&CellRef::new(2, 0)), "99");
    }

    #[test]
    fn test_paste_shifts_formula_references_by_offset() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(0, 0), "=B1").unwrap(); // A1

        let cell = core.grid.get(&CellRef::new(0, 0)).unwrap().clone();
        core.paste_cells(0, 1, 0, 0, &[(0, 0, cell)]).unwrap(); // paste to A2

        let pasted = core.grid.get(&CellRef::new(0, 1)).unwrap();
        match &pasted.contents {
            CellType::Script(s) => assert_eq!(s, "B2"),
            _ => panic!("Expected script cell"),
        }
    }

    #[test]
    fn test_set_cell_over_spill_output_clears_spill_and_marks_source_dirty() {
        let mut core = Document::new();

        core.set_cell_from_input(CellRef::new(1, 0), "1").unwrap(); // B1
        core.set_cell_from_input(CellRef::new(1, 1), "2").unwrap(); // B2
        core.set_cell_from_input(CellRef::new(1, 2), "3").unwrap(); // B3
        core.set_cell_from_input(CellRef::new(0, 0), "=VEC(B1:B3)")
            .unwrap(); // A1 spills to A2:A3
        core.evaluate_all_cells();

        let spill_output = CellRef::new(0, 1); // A2
        assert!(core.spill_sources.contains_key(&spill_output));

        core.set_cell_from_input(spill_output.clone(), "99").unwrap();

        assert!(!core.spill_sources.contains_key(&spill_output));
        let source = CellRef::new(0, 0);
        let source_cell = core.grid.get(&source).unwrap();
        assert!(source_cell.dirty);
    }

    #[test]
    fn test_undo_restores_spill_state_for_formula_source() {
        let mut core = Document::new();

        core.set_cell_from_input(CellRef::new(1, 0), "1").unwrap(); // B1
        core.set_cell_from_input(CellRef::new(1, 1), "2").unwrap(); // B2
        core.set_cell_from_input(CellRef::new(1, 2), "3").unwrap(); // B3
        core.set_cell_from_input(CellRef::new(0, 0), "=VEC(B1:B3)")
            .unwrap(); // A1 spills to A2:A3
        let _ = core.get_cell_display(&CellRef::new(0, 0));
        assert!(core.spill_sources.contains_key(&CellRef::new(0, 1)));

        core.set_cell_from_input(CellRef::new(0, 0), "99").unwrap();
        assert!(!core.spill_sources.contains_key(&CellRef::new(0, 1)));

        core.undo().unwrap();
        assert_eq!(core.get_cell_display(&CellRef::new(0, 0)), "1");
        assert!(core.spill_sources.contains_key(&CellRef::new(0, 1)));
        assert_eq!(core.get_cell_display(&CellRef::new(0, 1)), "2");
    }

    #[test]
    fn test_redo_clears_spill_state_when_restoring_scalar() {
        let mut core = Document::new();

        core.set_cell_from_input(CellRef::new(1, 0), "1").unwrap(); // B1
        core.set_cell_from_input(CellRef::new(1, 1), "2").unwrap(); // B2
        core.set_cell_from_input(CellRef::new(1, 2), "3").unwrap(); // B3
        core.set_cell_from_input(CellRef::new(0, 0), "=VEC(B1:B3)")
            .unwrap(); // A1 spills to A2:A3
        let _ = core.get_cell_display(&CellRef::new(0, 0));

        core.set_cell_from_input(CellRef::new(0, 0), "99").unwrap();
        core.undo().unwrap();
        // Recreate spill state before redo.
        let _ = core.get_cell_display(&CellRef::new(0, 0));
        assert!(core.spill_sources.contains_key(&CellRef::new(0, 1)));

        core.redo().unwrap();
        assert_eq!(core.get_cell_display(&CellRef::new(0, 0)), "99");
        assert!(!core.spill_sources.contains_key(&CellRef::new(0, 1)));
        assert_eq!(core.get_cell_display(&CellRef::new(0, 1)), "");
    }

    #[test]
    fn test_undo_after_save_marks_document_modified() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(0, 0), "1").unwrap();

        let save_path = std::env::temp_dir().join(format!(
            "gridline_undo_modified_{}_{}_{:?}.grd",
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
        let _cleanup = Cleanup(save_path.clone());
        core.file_path = Some(save_path);
        core.save_file().unwrap();
        assert!(!core.modified);

        core.undo().unwrap();
        assert!(core.modified);
        assert!(core.grid.get(&CellRef::new(0, 0)).is_none());
    }

    #[test]
    fn test_redo_after_save_marks_document_modified() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(0, 0), "1").unwrap();

        let save_path = std::env::temp_dir().join(format!(
            "gridline_redo_modified_{}_{}_{:?}.grd",
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
        let _cleanup = Cleanup(save_path.clone());
        core.file_path = Some(save_path);
        core.save_file().unwrap();
        assert!(!core.modified);

        core.undo().unwrap();
        core.save_file().unwrap();
        assert!(!core.modified);

        core.redo().unwrap();
        assert!(core.modified);
        assert_eq!(core.get_cell_display(&CellRef::new(0, 0)), "1");
    }

    #[test]
    fn test_paste_rejects_circular_dependency_transactionally() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(1, 0), "7").unwrap(); // B1 baseline

        let cycle_cell = Cell::new_script("A1");
        let result = core.paste_cells(0, 0, 0, 0, &[(0, 0, cycle_cell)]);
        assert!(matches!(result, Err(GridlineError::CircularDependency)));

        // Target was untouched.
        assert!(core.grid.get(&CellRef::new(0, 0)).is_none());
        // Existing unrelated cells remain unchanged.
        assert_eq!(core.get_cell_display(&CellRef::new(1, 0)), "7");
    }
}
