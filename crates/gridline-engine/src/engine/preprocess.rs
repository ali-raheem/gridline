//! Formula preprocessing and reference transformation.
//!
//! Before formulas can be evaluated by Rhai, cell references like `A1` must
//! be transformed into function calls like `CELL(0, 0)`. This module handles:
//!
//! - **Preprocessing**: Converting `A1` → `CELL(0, 0)` and `@A1` → `VALUE(0, 0)`
//! - **Range functions**: Converting `SUM(A1:B5)` → `SUM_RANGE(0, 0, 1, 4)` (col/row)
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
    let mut replacements: Vec<String> = Vec::new();
    // Handle range functions like SUM(A1:B5, ...)
    let with_placeholders = crate::builtins::range_fn_re()
        .replace_all(formula, |caps: &regex::Captures| {
            let func_name = &caps[1];
            let start_ref = &caps[2];
            let end_ref = &caps[3];
            let rest_args = caps.get(4).map(|m| m.as_str()).unwrap_or("");

            let new_start = shift_single_ref(start_ref, op);
            let new_end = shift_single_ref(end_ref, op);

            // If either ref became #REF!, return #REF!
            if new_start == "#REF!" || new_end == "#REF!" {
                let idx = replacements.len();
                replacements.push("#REF!".to_string());
                return format!("@@@{}@@@", idx);
            }

            let idx = replacements.len();
            replacements.push(format!(
                "{}({}:{}{})",
                func_name, new_start, new_end, rest_args
            ));
            format!("@@@{}@@@", idx)
        })
        .to_string();

    // Now shift individual cell references
    let shifted = shift_cell_refs_outside_strings(&with_placeholders, op);
    if replacements.is_empty() {
        return shifted;
    }

    let mut restored = shifted;
    for (idx, replacement) in replacements.into_iter().enumerate() {
        let placeholder = format!("@@@{}@@@", idx);
        restored = restored.replace(&placeholder, &replacement);
    }
    restored
}

/// Offset all cell references in a formula by a relative column/row delta.
/// Used by copy/paste so pasted formulas preserve relative references.
///
/// Rules:
/// - `A1` offset by (+1, +2) becomes `B3`
/// - `@A1` offset by (+1, +2) becomes `@B3`
/// - range refs are offset on both ends: `SUM(A1:B2)` -> `SUM(B3:C4)`
/// - refs that move out of bounds become `#REF!`
pub fn offset_formula_references(formula: &str, delta_col: isize, delta_row: isize) -> String {
    if delta_col == 0 && delta_row == 0 {
        return formula.to_string();
    }

    let mut replacements: Vec<String> = Vec::new();
    let with_placeholders = crate::builtins::range_fn_re()
        .replace_all(formula, |caps: &regex::Captures| {
            let func_name = &caps[1];
            let start_ref = &caps[2];
            let end_ref = &caps[3];
            let rest_args = caps.get(4).map(|m| m.as_str()).unwrap_or("");

            let new_start = offset_single_ref(start_ref, delta_col, delta_row);
            let new_end = offset_single_ref(end_ref, delta_col, delta_row);

            let idx = replacements.len();
            if new_start == "#REF!" || new_end == "#REF!" {
                replacements.push("#REF!".to_string());
            } else {
                replacements.push(format!(
                    "{}({}:{}{})",
                    func_name, new_start, new_end, rest_args
                ));
            }
            format!("@@@{}@@@", idx)
        })
        .to_string();

    let shifted = offset_cell_refs_outside_strings(&with_placeholders, delta_col, delta_row);
    if replacements.is_empty() {
        return shifted;
    }

    let mut restored = shifted;
    for (idx, replacement) in replacements.into_iter().enumerate() {
        let placeholder = format!("@@@{}@@@", idx);
        restored = restored.replace(&placeholder, &replacement);
    }
    restored
}

fn shift_single_ref(cell_ref_str: &str, op: ShiftOperation) -> String {
    let Some(cr) = CellRef::from_str(cell_ref_str) else {
        return cell_ref_str.to_string();
    };

    match op {
        ShiftOperation::InsertRow(at_row) => {
            if cr.row >= at_row {
                CellRef::new(cr.col, cr.row + 1).to_string()
            } else {
                cr.to_string()
            }
        }
        ShiftOperation::DeleteRow(at_row) => {
            if cr.row == at_row {
                "#REF!".to_string()
            } else if cr.row > at_row {
                CellRef::new(cr.col, cr.row - 1).to_string()
            } else {
                cr.to_string()
            }
        }
        ShiftOperation::InsertColumn(at_col) => {
            if cr.col >= at_col {
                CellRef::new(cr.col + 1, cr.row).to_string()
            } else {
                cr.to_string()
            }
        }
        ShiftOperation::DeleteColumn(at_col) => {
            if cr.col == at_col {
                "#REF!".to_string()
            } else if cr.col > at_col {
                CellRef::new(cr.col - 1, cr.row).to_string()
            } else {
                cr.to_string()
            }
        }
    }
}

fn offset_single_ref(cell_ref_str: &str, delta_col: isize, delta_row: isize) -> String {
    let Some(cr) = CellRef::from_str(cell_ref_str) else {
        return cell_ref_str.to_string();
    };

    let new_col = cr.col as isize + delta_col;
    let new_row = cr.row as isize + delta_row;
    if new_col < 0 || new_row < 0 {
        return "#REF!".to_string();
    }

    CellRef::new(new_col as usize, new_row as usize).to_string()
}

fn shift_cell_refs_outside_strings(script: &str, op: ShiftOperation) -> String {
    let cell_re = Regex::new(r"\b([A-Za-z]+)([0-9]+)\b").unwrap();
    let value_re = Regex::new(r"@([A-Za-z]+)([0-9]+)\b").unwrap();

    let shift_cells = |seg: &str| {
        // First handle @-prefixed refs using placeholders to avoid double-shifting.
        let mut value_refs: Vec<String> = Vec::new();
        let seg = value_re
            .replace_all(seg, |caps: &regex::Captures| {
                let cell_ref = format!("{}{}", &caps[1], &caps[2]);
                let shifted = shift_single_ref(&cell_ref, op);
                let idx = value_refs.len();
                if shifted == "#REF!" {
                    value_refs.push(shifted);
                } else {
                    value_refs.push(format!("@{}", shifted));
                }
                format!("__VALUE_REF_{}__", idx)
            })
            .to_string();

        // Then handle regular refs
        let shifted = cell_re
            .replace_all(&seg, |caps: &regex::Captures| {
                let cell_ref = format!("{}{}", &caps[1], &caps[2]);
                shift_single_ref(&cell_ref, op)
            })
            .to_string();

        if value_refs.is_empty() {
            return shifted;
        }

        let mut restored = shifted;
        for (idx, value_ref) in value_refs.into_iter().enumerate() {
            restored = restored.replace(&format!("__VALUE_REF_{}__", idx), &value_ref);
        }
        restored
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

fn offset_cell_refs_outside_strings(script: &str, delta_col: isize, delta_row: isize) -> String {
    let cell_re = Regex::new(r"\b([A-Za-z]+)([0-9]+)\b").unwrap();
    let value_re = Regex::new(r"@([A-Za-z]+)([0-9]+)\b").unwrap();

    let offset_cells = |seg: &str| {
        let mut value_refs: Vec<String> = Vec::new();
        let seg = value_re
            .replace_all(seg, |caps: &regex::Captures| {
                let cell_ref = format!("{}{}", &caps[1], &caps[2]);
                let shifted = offset_single_ref(&cell_ref, delta_col, delta_row);
                let idx = value_refs.len();
                if shifted == "#REF!" {
                    value_refs.push(shifted);
                } else {
                    value_refs.push(format!("@{}", shifted));
                }
                format!("__VALUE_REF_{}__", idx)
            })
            .to_string();

        let shifted = cell_re
            .replace_all(&seg, |caps: &regex::Captures| {
                let cell_ref = format!("{}{}", &caps[1], &caps[2]);
                offset_single_ref(&cell_ref, delta_col, delta_row)
            })
            .to_string();

        if value_refs.is_empty() {
            return shifted;
        }

        let mut restored = shifted;
        for (idx, value_ref) in value_refs.into_iter().enumerate() {
            restored = restored.replace(&format!("__VALUE_REF_{}__", idx), &value_ref);
        }
        restored
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
            out.push_str(&offset_cells(&script[seg_start..i]));
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
            out.push_str(&offset_cells(&script[seg_start..]));
        }
    }

    out
}

/// Replace cell references like "A1" with Rhai function calls like "CELL(0, 0)".
/// Typed refs like "@A1" become "VALUE(0, 0)" (returns Dynamic).
/// Also transforms range functions like SUM(A1:B5, ...) into SUM_RANGE(0, 0, 1, 4, ...).
pub fn preprocess_script(script: &str) -> String {
    preprocess_script_with_context(script, None)
}

/// Preprocess script with optional current cell context for ROW()/COL().
/// When context is provided, ROW() and COL() are replaced with 1-based row/col values (row/col ordering is unchanged).
// NOTE: builtin coordinate order is col/row.
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
                    rhai_name, start.col, start.row, end.col, end.row, rest_args
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
                    format!("VALUE({}, {})", cr.col, cr.row)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        cell_re
            .replace_all(&seg, |caps: &regex::Captures| {
                let cell_ref = format!("{}{}", &caps[1], &caps[2]);
                if let Some(cr) = CellRef::from_str(&cell_ref) {
                    format!("CELL({}, {})", cr.col, cr.row)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shift_formula_references_preserves_paren() {
        let formula = "VEC(A1:A100)";
        let shifted = shift_formula_references(formula, ShiftOperation::InsertColumn(0));
        assert_eq!(shifted, "VEC(B1:B100)");
    }

    #[test]
    fn test_shift_formula_references_mixed_range_and_cell() {
        let formula = "SUM(A1:A3) + B1";
        let shifted = shift_formula_references(formula, ShiftOperation::InsertColumn(0));
        assert_eq!(shifted, "SUM(B1:B3) + C1");
    }

    #[test]
    fn test_shift_formula_references_vec_and_cell() {
        let formula = "VEC(A1:A10) + B1";
        let shifted = shift_formula_references(formula, ShiftOperation::InsertColumn(0));
        assert_eq!(shifted, "VEC(B1:B10) + C1");
    }

    #[test]
    fn test_offset_formula_references_positive_delta() {
        let formula = "SUM(A1:B2) + @C3 + D4";
        let shifted = offset_formula_references(formula, 1, 2);
        assert_eq!(shifted, "SUM(B3:C4) + @D5 + E6");
    }

    #[test]
    fn test_offset_formula_references_out_of_bounds() {
        let formula = "A1 + @B2";
        let shifted = offset_formula_references(formula, -1, 0);
        assert_eq!(shifted, "#REF! + @A2");
    }
}
