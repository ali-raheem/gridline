use regex::Regex;

use super::cell_ref::CellRef;

/// Replace cell references like "A1" with Rhai function calls like "cell(0, 0)".
/// Typed refs like "@A1" become "value(0, 0)" (returns Dynamic).
/// Also transforms range functions like SUM(A1:B5, ...) into sum_range(0, 0, 4, 1, ...).
pub fn preprocess_script(script: &str) -> String {
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
