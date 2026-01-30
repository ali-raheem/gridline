use std::collections::HashSet;

use super::{CellRef, Grid};

/// Detect circular dependencies starting from a cell.
/// Returns Some(cycle_path) if a cycle is found, None otherwise.
pub fn detect_cycle(start: &CellRef, grid: &Grid) -> Option<Vec<CellRef>> {
    let mut visiting = HashSet::new();
    let mut path = Vec::new();

    if detect_cycle_dfs(start, grid, &mut visiting, &mut path) {
        Some(path)
    } else {
        None
    }
}

fn detect_cycle_dfs(
    current: &CellRef,
    grid: &Grid,
    visiting: &mut HashSet<CellRef>,
    path: &mut Vec<CellRef>,
) -> bool {
    if visiting.contains(current) {
        path.push(current.clone());
        return true;
    }

    let deps = match grid.get(current) {
        Some(entry) => entry.depends_on.clone(),
        None => return false,
    };

    visiting.insert(current.clone());
    path.push(current.clone());

    for dep in &deps {
        if detect_cycle_dfs(dep, grid, visiting, path) {
            return true;
        }
    }

    path.pop();
    visiting.remove(current);
    false
}
