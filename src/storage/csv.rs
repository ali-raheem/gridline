//! CSV import/export functionality

use crate::error::Result;
use gridline_engine::engine::{Cell, CellRef, Grid};
use std::io::Write;
use std::path::Path;

/// Parse a CSV file into cells, starting at the given offset
pub fn parse_csv(path: &Path, start_row: usize, start_col: usize) -> Result<Vec<(CellRef, Cell)>> {
    let content = std::fs::read_to_string(path)?;
    let mut cells = Vec::new();

    for (row_idx, line) in content.lines().enumerate() {
        for (col_idx, field) in parse_csv_line(line).into_iter().enumerate() {
            if field.is_empty() {
                continue;
            }
            let cell_ref = CellRef::new(start_row + row_idx, start_col + col_idx);
            let cell = parse_csv_field(&field);
            cells.push((cell_ref, cell));
        }
    }

    Ok(cells)
}

/// Parse a single CSV line, handling quoted fields
pub(crate) fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                // Check for escaped quote
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => {
                    fields.push(current.trim().to_string());
                    current = String::new();
                }
                _ => current.push(c),
            }
        }
    }
    fields.push(current.trim().to_string());
    fields
}

/// Parse a CSV field into an appropriate Cell type
/// - Empty string -> skip (handled by caller)
/// - Valid number -> Number (unless it has leading zeros like "007")
/// - Otherwise -> Text
pub(crate) fn parse_csv_field(field: &str) -> Cell {
    let trimmed = field.trim();

    if trimmed.is_empty() {
        return Cell::new_empty();
    }

    // Preserve strings that look like numbers but have leading zeros (e.g., "007", "00123")
    // unless they're just "0" or start with "0."
    if trimmed.starts_with('0')
        && trimmed.len() > 1
        && !trimmed.starts_with("0.")
        && trimmed
            .chars()
            .skip(1)
            .next()
            .map_or(false, |c| c.is_ascii_digit())
    {
        return Cell::new_text(trimmed);
    }

    // Try parsing as number
    if let Ok(n) = trimmed.parse::<f64>() {
        return Cell::new_number(n);
    }

    Cell::new_text(trimmed)
}

/// Export grid data to CSV format
pub fn write_csv(
    path: &Path,
    grid: &Grid,
    range: Option<((usize, usize), (usize, usize))>,
) -> Result<()> {
    let (min_row, min_col, max_row, max_col) = if let Some(((r1, c1), (r2, c2))) = range {
        (r1, c1, r2, c2)
    } else {
        // Auto-detect bounds from data
        let mut min_row = usize::MAX;
        let mut min_col = usize::MAX;
        let mut max_row = 0usize;
        let mut max_col = 0usize;

        for entry in grid.iter() {
            let cell_ref = entry.key();
            min_row = min_row.min(cell_ref.row);
            min_col = min_col.min(cell_ref.col);
            max_row = max_row.max(cell_ref.row);
            max_col = max_col.max(cell_ref.col);
        }

        if min_row == usize::MAX {
            // Empty grid
            return Ok(());
        }

        (min_row, min_col, max_row, max_col)
    };

    let mut file = std::fs::File::create(path)?;

    for row in min_row..=max_row {
        let mut row_fields = Vec::new();
        for col in min_col..=max_col {
            let cell_ref = CellRef::new(row, col);
            let value = if let Some(cell) = grid.get(&cell_ref) {
                cell_display_value(&cell)
            } else {
                String::new()
            };
            row_fields.push(escape_csv_field(&value));
        }
        writeln!(file, "{}", row_fields.join(","))?;
    }

    Ok(())
}

/// Get the display value of a cell for CSV export
fn cell_display_value(cell: &Cell) -> String {
    use gridline_engine::engine::CellType;
    match &cell.contents {
        CellType::Empty => String::new(),
        CellType::Text(s) => s.clone(),
        CellType::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }
        }
        CellType::Script(s) => {
            // For scripts, export the cached value if available, otherwise the formula
            if let Some(ref cached) = cell.cached_value {
                cached.clone()
            } else {
                format!("={}", s)
            }
        }
    }
}

/// Escape a field for CSV output
fn escape_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Core;

    #[test]
    fn test_parse_csv_line_simple() {
        assert_eq!(parse_csv_line("a,b,c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_csv_line_quoted() {
        assert_eq!(
            parse_csv_line(r#"a,"hello, world",c"#),
            vec!["a", "hello, world", "c"]
        );
    }

    #[test]
    fn test_parse_csv_line_escaped_quotes() {
        assert_eq!(
            parse_csv_line(r#"a,"say ""hello""",c"#),
            vec!["a", r#"say "hello""#, "c"]
        );
    }

    #[test]
    fn test_escape_csv_field() {
        assert_eq!(escape_csv_field("simple"), "simple");
        assert_eq!(escape_csv_field("with,comma"), "\"with,comma\"");
        assert_eq!(escape_csv_field("with\"quote"), "\"with\"\"quote\"");
    }

    #[test]
    fn test_parse_csv_field_number() {
        let cell = parse_csv_field("42");
        assert!(matches!(cell.contents, gridline_engine::engine::CellType::Number(n) if n == 42.0));
    }

    #[test]
    fn test_parse_csv_field_leading_zero() {
        let cell = parse_csv_field("007");
        assert!(
            matches!(cell.contents, gridline_engine::engine::CellType::Text(ref s) if s == "007")
        );
    }

    #[test]
    fn test_parse_csv_field_zero() {
        let cell = parse_csv_field("0");
        assert!(matches!(cell.contents, gridline_engine::engine::CellType::Number(n) if n == 0.0));
    }

    #[test]
    fn test_import_csv_invalidates_dependents() {
        let mut core = Core::new();
        core.set_cell_from_input(CellRef::new(0, 0), "1").unwrap(); // A1
        core.set_cell_from_input(CellRef::new(0, 1), "=A1 + 1").unwrap(); // B1

        let display_before = core.get_cell_display(&CellRef::new(0, 1));
        assert_eq!(display_before, "2");

        core.import_csv_raw("5", 0, 0).unwrap(); // overwrite A1

        let display_after = core.get_cell_display(&CellRef::new(0, 1));
        assert_eq!(display_after, "6");
    }
}
