//! Parser for .grd file format

use crate::error::{GridlineError, Result};
use gridline_engine::engine::{Cell, CellRef, Grid};
use std::fs;
use std::path::Path;

/// Parse a .grd file and return a Grid
pub fn parse_grd(path: &Path) -> Result<Grid> {
    let content = fs::read_to_string(path)?;
    parse_grd_content(&content)
}

/// Parse .grd content from a string
pub fn parse_grd_content(content: &str) -> Result<Grid> {
    let grid: Grid = std::sync::Arc::new(dashmap::DashMap::new());

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse "CELLREF: VALUE" format
        let Some((cell_ref_str, value_str)) = line.split_once(':') else {
            return Err(GridlineError::Parse {
                line: line_num + 1,
                message: "Expected 'CELLREF: VALUE' format".to_string(),
            });
        };

        let cell_ref_str = cell_ref_str.trim();
        let value_str = value_str.trim();

        let cell_ref = CellRef::from_str(cell_ref_str).ok_or_else(|| GridlineError::Parse {
            line: line_num + 1,
            message: format!("Invalid cell reference: {}", cell_ref_str),
        })?;

        let cell = parse_cell_value(value_str, line_num + 1)?;
        grid.insert(cell_ref, cell);
    }

    Ok(grid)
}

/// Parse a cell value string into a Cell
fn parse_cell_value(value: &str, line_num: usize) -> Result<Cell> {
    let value = value.trim();

    if value.is_empty() {
        return Ok(Cell::new_empty());
    }

    // Formula: starts with '='
    if let Some(formula) = value.strip_prefix('=') {
        return Ok(Cell::new_script(formula));
    }

    // Quoted string: starts and ends with '"'
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        let text = &value[1..value.len() - 1];
        let text = unescape_grd_text(text);
        return Ok(Cell::new_text(&text));
    }

    // Try to parse as number
    if let Ok(n) = value.parse::<f64>() {
        return Ok(Cell::new_number(n));
    }

    Err(GridlineError::Parse {
        line: line_num,
        message: format!("Invalid value: {}. Use quotes for text.", value),
    })
}

fn unescape_grd_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    _ => {
                        out.push('\\');
                        out.push(next);
                    }
                }
            } else {
                out.push('\\');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridline_engine::engine::CellType;

    #[test]
    fn test_parse_number() {
        let content = "A1: 42";
        let grid = parse_grd_content(content).unwrap();
        let cell = grid.get(&CellRef::new(0, 0)).unwrap();
        match &cell.contents {
            CellType::Number(n) => assert_eq!(*n, 42.0),
            _ => panic!("Expected number"),
        }
    }

    #[test]
    fn test_parse_text() {
        let content = r#"A1: "Hello""#;
        let grid = parse_grd_content(content).unwrap();
        let cell = grid.get(&CellRef::new(0, 0)).unwrap();
        match &cell.contents {
            CellType::Text(s) => assert_eq!(s, "Hello"),
            _ => panic!("Expected text"),
        }
    }

    #[test]
    fn test_parse_text_escaped_quotes() {
        let content = r#"A1: "He said \"hi\"""#;
        let grid = parse_grd_content(content).unwrap();
        let cell = grid.get(&CellRef::new(0, 0)).unwrap();
        match &cell.contents {
            CellType::Text(s) => assert_eq!(s, "He said \"hi\""),
            _ => panic!("Expected text"),
        }
    }

    #[test]
    fn test_parse_formula() {
        let content = "A1: =B1 + C1";
        let grid = parse_grd_content(content).unwrap();
        let cell = grid.get(&CellRef::new(0, 0)).unwrap();
        match &cell.contents {
            CellType::Script(s) => assert_eq!(s, "B1 + C1"),
            _ => panic!("Expected script"),
        }
    }

    #[test]
    fn test_parse_multiple_cells() {
        let content = r#"
# Test spreadsheet
A1: 100
A2: 200
A3: "Total"
B3: =A1 + A2
"#;
        let grid = parse_grd_content(content).unwrap();
        assert!(grid.contains_key(&CellRef::new(0, 0))); // A1
        assert!(grid.contains_key(&CellRef::new(0, 1))); // A2
        assert!(grid.contains_key(&CellRef::new(0, 2))); // A3
        assert!(grid.contains_key(&CellRef::new(1, 2))); // B3
    }

    #[test]
    fn test_skip_comments_and_empty_lines() {
        let content = r#"
# This is a comment
A1: 42

# Another comment

B1: 100
"#;
        let grid = parse_grd_content(content).unwrap();
        assert_eq!(grid.len(), 2);
    }
}
