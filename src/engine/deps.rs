use regex::Regex;

use super::cell_ref::CellRef;

/// Extract all cell references from a script as dependencies.
pub fn extract_dependencies(script: &str) -> Vec<CellRef> {
    let mut deps = Vec::new();

    // Ignore references inside string literals.
    let script = strip_string_literals(script);

    // Match range functions like SUM(A1:B5, ...)
    let range_re = crate::builtins::range_fn_re();

    // First, remove range function calls from the script to avoid double-counting
    let script_without_ranges = range_re.replace_all(&script, "").to_string();

    // Extract dependencies from ranges
    for caps in range_re.captures_iter(&script) {
        if let (Some(start), Some(end)) = (CellRef::from_str(&caps[2]), CellRef::from_str(&caps[3]))
        {
            let min_row = start.row.min(end.row);
            let max_row = start.row.max(end.row);
            let min_col = start.col.min(end.col);
            let max_col = start.col.max(end.col);
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    deps.push(CellRef::new(row, col));
                }
            }
        }
    }

    // Match individual cell references like A1, B2, etc.
    let cell_re = Regex::new(r"\b([A-Za-z]+)([0-9]+)\b").unwrap();

    for caps in cell_re.captures_iter(&script_without_ranges) {
        let cell_ref = format!("{}{}", &caps[1], &caps[2]);
        if let Some(cr) = CellRef::from_str(&cell_ref) {
            deps.push(cr);
        }
    }

    deps
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

/// Parse a cell range like "A1:B5" and return (start_row, start_col, end_row, end_col).
pub fn parse_range(range: &str) -> Option<(usize, usize, usize, usize)> {
    let parts: Vec<&str> = range.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let start = CellRef::from_str(parts[0])?;
    let end = CellRef::from_str(parts[1])?;
    Some((start.row, start.col, end.row, end.col))
}
