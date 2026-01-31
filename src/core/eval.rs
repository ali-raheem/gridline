use super::Core;
use gridline_engine::engine::{
    CellRef, CellType, detect_cycle, eval_with_functions, format_dynamic, format_number,
    preprocess_script_with_context,
};
use rhai::Dynamic;

impl Core {
    /// Get the display value for a cell
    pub fn get_cell_display(&mut self, cell_ref: &CellRef) -> String {
        // Check if this is a spill cell (value is in shared spill_map)
        if self.spill_sources.contains_key(cell_ref) {
            if let Some(val) = self.spill_map.get(cell_ref) {
                return format_dynamic(&val);
            }
            return "#SPILL?".to_string(); // Orphaned spill cell
        }

        let Some(cell) = self.grid.get(cell_ref) else {
            // Also check spill_map for cells that aren't in spill_sources
            // (the source cell itself stores its first value there too)
            if let Some(val) = self.spill_map.get(cell_ref) {
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

                match eval_with_functions(&self.engine, &processed, self.custom_ast.as_ref()) {
                    Ok(result) => {
                        if result.is_array() {
                            self.handle_array_spill(cell_ref, result)
                        } else {
                            // Store in computed_map so other formulas can reference this value
                            self.computed_map.insert(cell_ref.clone(), result.clone());
                            let display = format_dynamic(&result);
                            // Cache the result and clear dirty flag
                            if let Some(mut cell) = self.grid.get_mut(cell_ref) {
                                cell.cached_value = Some(display.clone());
                                cell.dirty = false;
                            }
                            display
                        }
                    }
                    Err(_) => "#ERR!".to_string(),
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

            // Conflict if cell exists with content
            if let Some(cell) = self.grid.get(&spill_ref) {
                if !matches!(cell.contents, CellType::Empty) {
                    return "#SPILL!".to_string();
                }
            }

            // Conflict if another formula is spilling here
            if let Some(other_source) = self.spill_sources.get(&spill_ref) {
                if other_source != source {
                    return "#SPILL!".to_string();
                }
            }
        }

        // Clear old spill from this source
        self.clear_spill_from(source);

        // Store all array values in the shared spill_map
        // This makes them accessible to the engine for chained VEC calls
        for (i, val) in array.iter().enumerate() {
            let cell_ref = CellRef::new(source.row + i, source.col);
            self.spill_map.insert(cell_ref.clone(), val.clone());

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
        // Remove the source cell's value from spill_map
        self.spill_map.remove(source);

        // Remove all spill cells from this source
        let to_remove: Vec<CellRef> = self
            .spill_sources
            .iter()
            .filter(|(_, src)| *src == source)
            .map(|(cell, _)| cell.clone())
            .collect();

        for cell in to_remove {
            self.spill_sources.remove(&cell);
            self.spill_map.remove(&cell);
        }
    }
}
