//! Formula preprocessing and reference transformation.
//!
//! Before formulas can be evaluated by Rhai, cell references like `A1` must
//! be transformed into function calls like `cell(0, 0)`. This module handles:
//!
//! - **Preprocessing**: Converting `A1` → `cell(0, 0)` and `@A1` → `value(0, 0)`
//! - **Range functions**: Converting `SUM(A1:B5)` → `sum_range(0, 0, 4, 1)`
//! - **Reference shifting**: Adjusting references when rows/columns are inserted/deleted

use regex::Regex;

use super::cell_ref::CellRef;

/// Operation for shifting cell references in formulas.
#[derive(Clone, Copy, Debug)]
pub enum ShiftOperation {
    InsertRow(usize),
    DeleteRow(usize),
    InsertColumn(usize),
    DeleteColumn(usize),
}

/// Shift cell references in a formula when rows/cols are inserted/deleted.
/// Returns the updated formula string.
///
/// Rules:
/// - Insert row at R: refs to row >= R become row + 1
/// - Delete row at R: refs to row > R become row - 1; row == R becomes `#REF!`
/// - Same logic for columns
pub fn shift_formula_references(formula: &str, op: ShiftOperation) -> String {
    // Handle range functions like SUM(A1:B5, ...)
    let with_shifted_ranges = crate::builtins::range_fn_re()
        .replace_all(formula, |caps: &regex::Captures| {
            let func_name = &caps[1];
            let start_ref = &caps[2];
            let end_ref = &caps[3];
            let rest_args = caps.get(4).map(|m| m.as_str()).unwrap_or("");

            let new_start = shift_single_ref(start_ref, op);
            let new_end = shift_single_ref(end_ref, op);

            // If either ref became #REF!, return #REF!
            if new_start == "#REF!" || new_end == "#REF!" {
                return "#REF!".to_string();
            }

            format!("{}({}:{}{}", func_name, new_start, new_end, rest_args)
        })
        .to_string();

    // Now shift individual cell references
    shift_cell_refs_outside_strings(&with_shifted_ranges, op)
}

fn shift_single_ref(cell_ref_str: &str, op: ShiftOperation) -> String {
    let Some(cr) = CellRef::from_str(cell_ref_str) else {
        return cell_ref_str.to_string();
    };

    match op {
        ShiftOperation::InsertRow(at_row) => {
            if cr.row >= at_row {
                CellRef::new(cr.row + 1, cr.col).to_string()
            } else {
                cr.to_string()
            }
        }
        ShiftOperation::DeleteRow(at_row) => {
            if cr.row == at_row {
                "#REF!".to_string()
            } else if cr.row > at_row {
                CellRef::new(cr.row - 1, cr.col).to_string()
            } else {
                cr.to_string()
            }
        }
        ShiftOperation::InsertColumn(at_col) => {
            if cr.col >= at_col {
                CellRef::new(cr.row, cr.col + 1).to_string()
            } else {
                cr.to_string()
            }
        }
        ShiftOperation::DeleteColumn(at_col) => {
            if cr.col == at_col {
                "#REF!".to_string()
            } else if cr.col > at_col {
                CellRef::new(cr.row, cr.col - 1).to_string()
            } else {
                cr.to_string()
            }
        }
    }
}

fn shift_cell_refs_outside_strings(script: &str, op: ShiftOperation) -> String {
    let cell_re = Regex::new(r"\b([A-Za-z]+)([0-9]+)\b").unwrap();
    let value_re = Regex::new(r"@([A-Za-z]+)([0-9]+)\b").unwrap();

    let shift_cells = |seg: &str| {
        // First handle @-prefixed refs (value refs)
        let seg = value_re
            .replace_all(seg, |caps: &regex::Captures| {
                let cell_ref = format!("{}{}", &caps[1], &caps[2]);
                let shifted = shift_single_ref(&cell_ref, op);
                if shifted == "#REF!" {
                    shifted
                } else {
                    format!("@{}", shifted)
                }
            })
            .to_string();

        // Then handle regular refs
        cell_re
            .replace_all(&seg, |caps: &regex::Captures| {
                let cell_ref = format!("{}{}", &caps[1], &caps[2]);
                shift_single_ref(&cell_ref, op)
            })
            .to_string()
    };

    // Process outside of string literals
    let bytes = script.as_bytes();
    let mut out = String::new();
    let mut seg_start = 0;
    let mut in_string = false;
    let mut backslashes = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == b'\\' {
                backslashes += 1;
                i += 1;
                continue;
            }
            if b == b'"' && backslashes % 2 == 0 {
                out.push_str(&script[seg_start..=i]);
                in_string = false;
                seg_start = i + 1;
            }
            backslashes = 0;
            i += 1;
            continue;
        }

        if b == b'"' {
            out.push_str(&shift_cells(&script[seg_start..i]));
            in_string = true;
            seg_start = i;
            backslashes = 0;
            i += 1;
            continue;
        }

        i += 1;
    }

    if seg_start < script.len() {
        if in_string {
            out.push_str(&script[seg_start..]);
        } else {
            out.push_str(&shift_cells(&script[seg_start..]));
        }
    }

    out
}

/// Replace cell references like "A1" with Rhai function calls like "cell(0, 0)".
/// Typed refs like "@A1" become "value(0, 0)" (returns Dynamic).
/// Also transforms range functions like SUM(A1:B5, ...) into sum_range(0, 0, 4, 1, ...).
pub fn preprocess_script(script: &str) -> String {
    preprocess_script_with_context(script, None)
}

/// Preprocess script with optional current cell context for ROW()/COL().
/// When context is provided, ROW() and COL() are replaced with 1-based row/col values.
pub fn preprocess_script_with_context(script: &str, context: Option<&CellRef>) -> String {
    // First, replace ROW() and COL() if context is provided
    let script = if let Some(cell_ref) = context {
        let row_re = Regex::new(r"\bROW\(\s*\)").unwrap();
        let col_re = Regex::new(r"\bCOL\(\s*\)").unwrap();
        let script = row_re
            .replace_all(script, (cell_ref.row + 1).to_string())
            .to_string();
        col_re
            .replace_all(&script, (cell_ref.col + 1).to_string())
            .to_string()
    } else {
        script.to_string()
    };

    preprocess_script_inner(&script)
}

fn preprocess_script_inner(script: &str) -> String {
    let with_ranges = crate::builtins::range_fn_re()
        .replace_all(script, |caps: &regex::Captures| {
            let start_ref = &caps[2];
            let end_ref = &caps[3];
            let rest_args = caps.get(4).map(|m| m.as_str()).unwrap_or("");

            let Some(rhai_name) = crate::builtins::range_rhai_name(&caps[1]) else {
                return caps[0].to_string();
            };

            if let (Some(start), Some(end)) =
                (CellRef::from_str(start_ref), CellRef::from_str(end_ref))
            {
                format!(
                    "{}({}, {}, {}, {}{})",
                    rhai_name, start.row, start.col, end.row, end.col, rest_args
                )
            } else {
                caps[0].to_string()
            }
        })
        .to_string();

    replace_cell_refs_outside_strings(&with_ranges)
}

fn replace_cell_refs_outside_strings(script: &str) -> String {
    let cell_re = Regex::new(r"\b([A-Za-z]+)([0-9]+)\b").unwrap();
    let value_re = Regex::new(r"@([A-Za-z]+)([0-9]+)\b").unwrap();

    let replace_cells = |seg: &str| {
        let seg = value_re
            .replace_all(seg, |caps: &regex::Captures| {
                let cell_ref = format!("{}{}", &caps[1], &caps[2]);
                if let Some(cr) = CellRef::from_str(&cell_ref) {
                    format!("value({}, {})", cr.row, cr.col)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        cell_re
            .replace_all(&seg, |caps: &regex::Captures| {
                let cell_ref = format!("{}{}", &caps[1], &caps[2]);
                if let Some(cr) = CellRef::from_str(&cell_ref) {
                    format!("cell({}, {})", cr.row, cr.col)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string()
    };

    let bytes = script.as_bytes();
    let mut out = String::new();
    let mut seg_start = 0;
    let mut in_string = false;
    let mut backslashes = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == b'\\' {
                backslashes += 1;
                i += 1;
                continue;
            }
            if b == b'"' && backslashes.is_multiple_of(2) {
                out.push_str(&script[seg_start..=i]);
                in_string = false;
                seg_start = i + 1;
            }
            backslashes = 0;
            i += 1;
            continue;
        }

        if b == b'"' {
            out.push_str(&replace_cells(&script[seg_start..i]));
            in_string = true;
            seg_start = i;
            backslashes = 0;
            i += 1;
            continue;
        }

        i += 1;
    }

    if seg_start < script.len() {
        if in_string {
            out.push_str(&script[seg_start..]);
        } else {
            out.push_str(&replace_cells(&script[seg_start..]));
        }
    }

    out
}
