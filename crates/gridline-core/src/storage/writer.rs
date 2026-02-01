//! Writer for .grd file format

use crate::error::Result;
use gridline_engine::engine::{CellType, Grid};
use std::fs;
use std::path::Path;

/// Write a Grid to a .grd file
pub fn write_grd(path: &Path, grid: &Grid) -> Result<()> {
    let content = write_grd_content(grid);
    fs::write(path, content)?;
    Ok(())
}

/// Write a Grid to a .grd format string
pub fn write_grd_content(grid: &Grid) -> String {
    let mut lines = vec!["# Gridline Spreadsheet".to_string()];

    // Collect and sort cells by position for consistent output
    let mut cells: Vec<_> = grid.iter().collect();
    cells.sort_by(|a, b| {
        let a_key = a.key();
        let b_key = b.key();
        a_key.row.cmp(&b_key.row).then(a_key.col.cmp(&b_key.col))
    });

    for entry in cells {
        let cell_ref = entry.key();
        let cell = entry.value();

        let value_str = match &cell.contents {
            CellType::Empty => continue, // Skip empty cells
            CellType::Number(n) => n.to_string(),
            CellType::Text(s) => format!("\"{}\"", escape_grd_text(s)),
            CellType::Script(s) => format!("={}", s),
        };

        lines.push(format!("{}: {}", cell_ref, value_str));
    }

    lines.join("\n") + "\n"
}

fn escape_grd_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridline_engine::engine::{Cell, CellRef};

    #[test]
    fn test_write_number() {
        let grid: Grid = std::sync::Arc::new(dashmap::DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_number(42.0));
        let content = write_grd_content(&grid);
        assert!(content.contains("A1: 42"));
    }

    #[test]
    fn test_write_text() {
        let grid: Grid = std::sync::Arc::new(dashmap::DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_text("Hello"));
        let content = write_grd_content(&grid);
        assert!(content.contains("A1: \"Hello\""));
    }

    #[test]
    fn test_write_formula() {
        let grid: Grid = std::sync::Arc::new(dashmap::DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_script("B1 + C1"));
        let content = write_grd_content(&grid);
        assert!(content.contains("A1: =B1 + C1"));
    }

    #[test]
    fn test_skip_empty_cells() {
        let grid: Grid = std::sync::Arc::new(dashmap::DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_empty());
        grid.insert(CellRef::new(0, 1), Cell::new_number(42.0));
        let content = write_grd_content(&grid);
        assert!(!content.contains("A1:"));
        assert!(content.contains("B1: 42"));
    }

    #[test]
    fn test_sorted_output() {
        let grid: Grid = std::sync::Arc::new(dashmap::DashMap::new());
        grid.insert(CellRef::new(1, 1), Cell::new_number(3.0)); // B2
        grid.insert(CellRef::new(0, 0), Cell::new_number(1.0)); // A1
        grid.insert(CellRef::new(0, 1), Cell::new_number(2.0)); // B1
        let content = write_grd_content(&grid);
        let lines: Vec<_> = content.lines().collect();
        // After header, should be A1, B1, B2
        assert!(lines[1].starts_with("A1"));
        assert!(lines[2].starts_with("B1"));
        assert!(lines[3].starts_with("B2"));
    }
}
