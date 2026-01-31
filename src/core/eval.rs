use super::Core;
use gridline_engine::engine::{
    CellRef, CellType, detect_cycle, eval_with_functions_script, format_dynamic, format_number,
    preprocess_script_with_context,
};
use rhai::Dynamic;

impl Core {
    /// Get the display value for a cell
    pub fn get_cell_display(&mut self, cell_ref: &CellRef) -> String {
        // Check if this is a spill cell (value is in shared value_cache)
        if self.spill_sources.contains_key(cell_ref) {
            if let Some(val) = self.value_cache.get(cell_ref) {
                return format_dynamic(&val);
            }
            return "#SPILL?".to_string(); // Orphaned spill cell
        }

        let Some(cell) = self.grid.get(cell_ref) else {
            // Also check value_cache for cells that aren't in spill_sources
            // (the source cell itself stores its first value there too)
            if let Some(val) = self.value_cache.get(cell_ref) {
                return format_dynamic(&val);
            }
            return String::new();
        };

        match &cell.contents {
            CellType::Empty => String::new(),
            CellType::Text(s) => s.clone(),
            CellType::Number(n) => format_number(*n),
            CellType::Script(s) => {
                // Return cached value if not dirty
                if !cell.dirty
                    && let Some(ref cached) = cell.cached_value
                {
                    return cached.clone();
                }

                // Check for cycles
                if detect_cycle(cell_ref, &self.grid).is_some() {
                    return "#CYCLE!".to_string();
                }

                let processed = preprocess_script_with_context(s, Some(cell_ref));
                drop(cell);

                match eval_with_functions_script(&self.engine, &processed, self.custom_functions.as_deref()) {
                    Ok(result) => {
                        if result.is_array() {
                            self.handle_array_spill(cell_ref, result)
                        } else {
                            // Store in value_cache so other formulas can reference this value
                            self.value_cache.insert(cell_ref.clone(), result.clone());
                            let display = format_dynamic(&result);
                            // Cache the result and clear dirty flag
                            if let Some(mut cell) = self.grid.get_mut(cell_ref) {
                                cell.cached_value = Some(display.clone());
                                cell.dirty = false;
                            }
                            display
                        }
                    }
                    Err(e) => {
                        // Show first 50 chars of error for debugging (UTF-8 safe)
                        let mut chars = e.chars();
                        let prefix: String = chars.by_ref().take(50).collect();
                        let err_msg = if chars.next().is_some() {
                            format!("#ERR: {}...", prefix)
                        } else {
                            format!("#ERR: {}", prefix)
                        };
                        err_msg
                    }
                }
            }
        }
    }

    /// Handle array result - check conflicts and set up spill
    fn handle_array_spill(&mut self, source: &CellRef, result: Dynamic) -> String {
        let array: Vec<Dynamic> = result.into_array().unwrap();
        if array.is_empty() {
            return String::new();
        }

        // Check for conflicts in spill range
        for i in 1..array.len() {
            let spill_ref = CellRef::new(source.row + i, source.col);

            // Compute conflicts in a narrow scope so we can mutate after.
            let (has_cell_conflict, has_spill_conflict) = {
                let cell_conflict = self
                    .grid
                    .get(&spill_ref)
                    .is_some_and(|cell| !matches!(cell.contents, CellType::Empty));
                let spill_conflict = self
                    .spill_sources
                    .get(&spill_ref)
                    .is_some_and(|other_source| other_source != source);
                (cell_conflict, spill_conflict)
            };

            if has_cell_conflict || has_spill_conflict {
                self.clear_spill_from(source);
                return "#SPILL!".to_string();
            }
        }

        // Clear old spill from this source
        self.clear_spill_from(source);

        // Store all array values in the shared value_cache
        // This makes them accessible to the engine for chained VEC calls
        for (i, val) in array.iter().enumerate() {
            let cell_ref = CellRef::new(source.row + i, source.col);
            self.value_cache.insert(cell_ref.clone(), val.clone());

            // Register spill cells (skip index 0, that's the source cell)
            if i > 0 {
                self.spill_sources.insert(cell_ref, source.clone());
            }
        }

        // Format first value for display and cache
        let first = format_dynamic(&array[0]);

        // Cache the first value in the source cell
        if let Some(mut cell) = self.grid.get_mut(source) {
            cell.cached_value = Some(first.clone());
            cell.dirty = false;
        }

        first
    }

    /// Clear spill cells originating from a source
    pub(crate) fn clear_spill_from(&mut self, source: &CellRef) {
        // Remove the source cell's value from value_cache
        self.value_cache.remove(source);

        // Remove all spill cells from this source
        let to_remove: Vec<CellRef> = self
            .spill_sources
            .iter()
            .filter(|(_, src)| *src == source)
            .map(|(cell, _)| cell.clone())
            .collect();

        for cell in to_remove {
            self.spill_sources.remove(&cell);
            self.value_cache.remove(&cell);
        }
    }

    /// Evaluate all script cells in dependency order.
    /// This ensures that cells are computed before cells that depend on them.
    pub(crate) fn evaluate_all_cells(&mut self) {
        // Collect all script cells as a set for quick lookup
        let script_cells: std::collections::HashSet<CellRef> = self
            .grid
            .iter()
            .filter(|entry| matches!(entry.value().contents, CellType::Script(_)))
            .map(|entry| entry.key().clone())
            .collect();

        if script_cells.is_empty() {
            return;
        }

        // Build dependency info: for each cell, which script cells does it depend on?
        let mut cell_deps: std::collections::HashMap<CellRef, Vec<CellRef>> =
            std::collections::HashMap::new();
        for cell_ref in &script_cells {
            if let Some(cell) = self.grid.get(cell_ref) {
                // Only count dependencies that are script cells
                let script_deps: Vec<CellRef> = cell
                    .depends_on
                    .iter()
                    .filter(|dep| script_cells.contains(dep))
                    .cloned()
                    .collect();
                cell_deps.insert(cell_ref.clone(), script_deps);
            }
        }

        // Topological sort using Kahn's algorithm
        // in_degree = number of script cells this cell depends on
        let mut in_degree: std::collections::HashMap<CellRef, usize> =
            std::collections::HashMap::new();
        for (cell_ref, deps) in &cell_deps {
            in_degree.insert(cell_ref.clone(), deps.len());
        }

        // Start with cells that have no script cell dependencies
        let mut queue: std::collections::VecDeque<CellRef> = in_degree
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(cell, _)| cell.clone())
            .collect();

        let mut eval_order = Vec::new();

        while let Some(cell_ref) = queue.pop_front() {
            eval_order.push(cell_ref.clone());

            // For each cell that depends on this one, decrement its in-degree
            for (other_cell, deps) in &cell_deps {
                if deps.contains(&cell_ref) {
                    if let Some(count) = in_degree.get_mut(other_cell) {
                        *count = count.saturating_sub(1);
                        if *count == 0 && !eval_order.contains(other_cell) {
                            queue.push_back(other_cell.clone());
                        }
                    }
                }
            }
        }

        // If we couldn't order all cells (cycles), add remaining cells
        for cell_ref in &script_cells {
            if !eval_order.contains(cell_ref) {
                eval_order.push(cell_ref.clone());
            }
        }

        // Evaluate cells in dependency order
        for cell_ref in eval_order {
            let _ = self.get_cell_display(&cell_ref);
        }
    }

}
