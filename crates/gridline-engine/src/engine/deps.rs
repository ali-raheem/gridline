//! Dependency extraction from formula strings.
//!
//! Parses formula text to find all cell references (e.g., `A1`, `B2:C5`)
//! that the formula depends on. This is used to build the dependency graph
//! for cache invalidation and cycle detection.
//!
//! Handles:
//! - Simple cell references: `A1`, `B2`
//! - Range references in functions: `SUM(A1:B5)`
//! - Ignores references inside string literals

use regex::Regex;
use std::sync::OnceLock;

use super::cell_ref::CellRef;

const MAX_DEPENDENCY_RANGE_CELLS: usize = 1_000_000;

/// Extract all cell references from a script as dependencies.
pub fn extract_dependencies(script: &str) -> Vec<CellRef> {
    let mut deps = Vec::new();

    // Ignore references inside string literals.
    let script = strip_string_literals(script);

    // Match LOOKUP(value, search_range, return_range) — two ranges
    let lookup_re = crate::builtins::lookup_fn_re();
    let script_without_lookups = lookup_re.replace_all(&script, "").to_string();

    for caps in lookup_re.captures_iter(&script) {
        // Extract both search range (groups 2-3) and return range (groups 4-5)
        for (start_group, end_group) in [(2, 3), (4, 5)] {
            if let (Some(start), Some(end)) = (
                CellRef::from_str(&caps[start_group]),
                CellRef::from_str(&caps[end_group]),
            ) {
                let min_row = start.row.min(end.row);
                let max_row = start.row.max(end.row);
                let min_col = start.col.min(end.col);
                let max_col = start.col.max(end.col);

                let row_count = max_row - min_row + 1;
                let col_count = max_col - min_col + 1;
                let Some(cell_count) = row_count.checked_mul(col_count) else {
                    continue;
                };
                if cell_count > MAX_DEPENDENCY_RANGE_CELLS {
                    continue;
                }

                for row in min_row..=max_row {
                    for col in min_col..=max_col {
                        deps.push(CellRef::new(col, row));
                    }
                }
            }
        }
    }

    // Match range functions like SUM(A1:B5, ...)
    let range_re = crate::builtins::range_fn_re();

    // First, remove range function calls from the script to avoid double-counting
    let script_without_ranges = range_re.replace_all(&script_without_lookups, "").to_string();

    // Extract dependencies from ranges
    for caps in range_re.captures_iter(&script_without_lookups) {
        if let (Some(start), Some(end)) = (CellRef::from_str(&caps[2]), CellRef::from_str(&caps[3]))
        {
            let min_row = start.row.min(end.row);
            let max_row = start.row.max(end.row);
            let min_col = start.col.min(end.col);
            let max_col = start.col.max(end.col);

            let row_count = max_row - min_row + 1;
            let col_count = max_col - min_col + 1;
            let Some(cell_count) = row_count.checked_mul(col_count) else {
                continue;
            };
            if cell_count > MAX_DEPENDENCY_RANGE_CELLS {
                continue;
            }

            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    deps.push(CellRef::new(col, row));
                }
            }
        }
    }

    // Match individual cell references like A1, B2, etc.
    let cell_re = cell_ref_re();

    for caps in cell_re.captures_iter(&script_without_ranges) {
        let cell_ref = format!("{}{}", &caps[1], &caps[2]);
        if let Some(cr) = CellRef::from_str(&cell_ref) {
            deps.push(cr);
        }
    }

    deps
}

fn cell_ref_re() -> &'static Regex {
    static CELL_RE: OnceLock<Regex> = OnceLock::new();
    CELL_RE.get_or_init(|| {
        Regex::new(r"\b([A-Za-z]+)([0-9]+)\b")
            .expect("dependency cell reference regex must compile")
    })
}

fn strip_string_literals(script: &str) -> String {
    let mut out = String::with_capacity(script.len());
    let mut in_string = false;
    let mut escaped = false;

    for ch in script.chars() {
        if in_string {
            if escaped {
                escaped = false;
                out.push(' ');
                continue;
            }
            if ch == '\\' {
                escaped = true;
                out.push(' ');
                continue;
            }
            if ch == '"' {
                in_string = false;
                out.push('"');
            } else {
                out.push(' ');
            }
        } else if ch == '"' {
            in_string = true;
            out.push('"');
        } else {
            out.push(ch);
        }
    }

    out
}

/// Parse a cell range like "A1:B5" and return (start_col, start_row, end_col, end_row).
pub fn parse_range(range: &str) -> Option<(usize, usize, usize, usize)> {
    let parts: Vec<&str> = range.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let start = CellRef::from_str(parts[0])?;
    let end = CellRef::from_str(parts[1])?;
    Some((start.col, start.row, end.col, end.row))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_dependencies_skips_over_limit_ranges() {
        let deps = extract_dependencies("SUM(A1:A1000001)+B2");
        assert_eq!(deps, vec![CellRef::new(1, 1)]);
    }
}
