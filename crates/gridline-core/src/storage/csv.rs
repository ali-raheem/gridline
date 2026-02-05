//! CSV import/export functionality

use crate::document::Document;
use crate::error::Result;
use gridline_engine::engine::{Cell, CellRef};
use std::io::Write;
use std::path::Path;

/// Parse a CSV file into cells, starting at the given offset
pub fn parse_csv(path: &Path, start_col: usize, start_row: usize) -> Result<Vec<(CellRef, Cell)>> {
    let content = std::fs::read_to_string(path)?;
    let mut cells = Vec::new();

    for (row_idx, line) in content.lines().enumerate() {
        for (col_idx, field) in parse_csv_line(line).into_iter().enumerate() {
            if field.is_empty() {
                continue;
            }
            let cell_ref = CellRef::new(start_col + col_idx, start_row + row_idx);
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
    let mut field_was_quoted = false;
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
                '"' => {
                    in_quotes = true;
                    field_was_quoted = true;
                }
                ',' => {
                    if field_was_quoted {
                        fields.push(current.clone());
                    } else {
                        fields.push(current.trim().to_string());
                    }
                    current = String::new();
                    field_was_quoted = false;
                }
                _ => current.push(c),
            }
        }
    }
    if field_was_quoted {
        fields.push(current);
    } else {
        fields.push(current.trim().to_string());
    }
    fields
}

/// Parse a CSV field into an appropriate Cell type
/// - Empty string -> skip (handled by caller)
/// - Valid number -> Number (unless it has leading zeros like "007")
/// - Otherwise -> Text
pub(crate) fn parse_csv_field(field: &str) -> Cell {
    if field.is_empty() {
        return Cell::new_empty();
    }

    // Keep explicit surrounding whitespace (typically from quoted CSV fields).
    // This preserves values like "  hello  " exactly as text.
    let trimmed = field.trim();
    if field != trimmed {
        return Cell::new_text(field);
    }

    // Preserve strings that look like numbers but have leading zeros (e.g., "007", "00123")
    // unless they're just "0" or start with "0."
    if trimmed.starts_with('0')
        && trimmed.len() > 1
        && !trimmed.starts_with("0.")
        && trimmed.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
    {
        return Cell::new_text(trimmed);
    }

    // Try parsing as number
    if let Ok(n) = trimmed.parse::<f64>() {
        return Cell::new_number(n);
    }

    Cell::new_text(trimmed)
}

/// Export grid data to CSV format using evaluated display values.
pub fn write_csv(
    path: &Path,
    doc: &mut Document,
    range: Option<((usize, usize), (usize, usize))>,
) -> Result<()> {
    let (min_row, min_col, max_row, max_col) = if let Some(((c1, r1), (c2, r2))) = range {
        (r1, c1, r2, c2)
    } else {
        // Auto-detect bounds from data and cached spill values.
        let mut min_row = usize::MAX;
        let mut min_col = usize::MAX;
        let mut max_row = 0usize;
        let mut max_col = 0usize;

        for entry in doc.grid.iter() {
            let cell_ref = entry.key();
            min_row = min_row.min(cell_ref.row);
            min_col = min_col.min(cell_ref.col);
            max_row = max_row.max(cell_ref.row);
            max_col = max_col.max(cell_ref.col);
        }

        for entry in doc.value_cache.iter() {
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
            let cell_ref = CellRef::new(col, row);
            let value = doc.get_cell_display(&cell_ref);
            row_fields.push(escape_csv_field(&value));
        }
        writeln!(file, "{}", row_fields.join(","))?;
    }

    Ok(())
}

/// Escape a field for CSV output
fn escape_csv_field(field: &str) -> String {
    // Guard against CSV formula injection in spreadsheet apps.
    let first_non_space = field.trim_start_matches([' ', '\t']).chars().next();
    let safe_field = if matches!(first_non_space, Some('=' | '+' | '-' | '@')) {
        format!("'{}", field)
    } else {
        field.to_string()
    };

    if safe_field.contains(',')
        || safe_field.contains('"')
        || safe_field.contains('\n')
        || safe_field.contains('\r')
    {
        format!("\"{}\"", safe_field.replace('"', "\"\""))
    } else {
        safe_field
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

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
    fn test_parse_csv_line_quoted_preserves_whitespace() {
        assert_eq!(
            parse_csv_line(r#""  keep me  ",x"#),
            vec!["  keep me  ", "x"]
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
    fn test_escape_csv_field_formula_injection_with_leading_whitespace() {
        assert_eq!(escape_csv_field(" =1+1"), "' =1+1");
        assert_eq!(escape_csv_field("\t-2+3"), "'\t-2+3");
        assert_eq!(escape_csv_field(" \t@cmd"), "' \t@cmd");
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
    fn test_parse_csv_field_preserves_surrounding_whitespace() {
        let cell = parse_csv_field("  keep me  ");
        assert!(
            matches!(cell.contents, gridline_engine::engine::CellType::Text(ref s) if s == "  keep me  ")
        );
    }

    #[test]
    fn test_import_csv_invalidates_dependents() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(0, 0), "1").unwrap(); // A1
        core.set_cell_from_input(CellRef::new(1, 0), "=A1 + 1")
            .unwrap(); // B1

        let display_before = core.get_cell_display(&CellRef::new(1, 0));

        assert_eq!(display_before, "2");

        core.import_csv_raw("5", 0, 0).unwrap(); // overwrite A1

        let display_after = core.get_cell_display(&CellRef::new(1, 0));

        assert_eq!(display_after, "6");
    }

    #[test]
    fn test_export_csv_uses_evaluated_values() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(0, 0), "=1+2")
            .unwrap();
        core.set_cell_from_input(CellRef::new(1, 0), "text")
            .unwrap();

        let output_path = std::env::temp_dir().join(format!(
            "gridline_export_eval_{}_{}_{:?}.csv",
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
        let _cleanup = Cleanup(output_path.clone());

        write_csv(&output_path, &mut core, None).unwrap();
        let contents = std::fs::read_to_string(output_path).unwrap();
        assert_eq!(contents.trim_end(), "3,text");
    }

    #[test]
    fn test_export_csv_includes_spill_values() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(0, 0), "=SPILL(0..=9)")
            .unwrap();
        let _ = core.get_cell_display(&CellRef::new(0, 0));

        let output_path = std::env::temp_dir().join(format!(
            "gridline_export_spill_{}_{}_{:?}.csv",
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
        let _cleanup = Cleanup(output_path.clone());

        write_csv(&output_path, &mut core, None).unwrap();
        let contents = std::fs::read_to_string(output_path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 10);
        assert_eq!(lines.first().copied(), Some("0"));
        assert_eq!(lines.last().copied(), Some("9"));
    }

    #[test]
    fn test_export_csv_range_limits_spill_values() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(0, 0), "=SPILL(0..=9)")
            .unwrap();
        let _ = core.get_cell_display(&CellRef::new(0, 0));

        let output_path = std::env::temp_dir().join(format!(
            "gridline_export_spill_range_{}_{}_{:?}.csv",
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
        let _cleanup = Cleanup(output_path.clone());

        let range = Some(((0, 2), (0, 4)));
        write_csv(&output_path, &mut core, range).unwrap();
        let contents = std::fs::read_to_string(output_path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines, vec!["2", "3", "4"]);
    }

    #[test]
    fn test_escape_csv_formula_injection_prefixes_value() {
        let mut core = Document::new();
        core.set_cell_from_input(CellRef::new(0, 0), "\"=1+1\"")
            .unwrap();

        let output_path = std::env::temp_dir().join(format!(
            "gridline_export_formula_safety_{}_{}_{:?}.csv",
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
        let _cleanup = Cleanup(output_path.clone());

        write_csv(&output_path, &mut core, None).unwrap();
        let contents = std::fs::read_to_string(output_path).unwrap();
        assert_eq!(contents.trim_end(), "'=1+1");
    }
}
