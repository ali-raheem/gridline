//! Built-in spreadsheet functions (Rust) and their metadata.
//!
//! Conventions:
//! - Spreadsheet-facing built-in names are ALL CAPS (e.g. `SUM`, `AVG`).
//! - Built-ins rewrite to lowercase Rhai function names (e.g. `sum_range`).
//! - If you add a new built-in range function, update `RANGE_BUILTINS` and
//!   register its implementation in `register_builtins`.

use crate::engine::{CellRef, CellType, Grid, SpillMap, preprocess_script};
use crate::plot::{PlotKind, PlotSpec, format_plot_spec};
use regex::Regex;
use rhai::{Dynamic, Engine, FnPtr, NativeCallContext};
use std::sync::{Arc, OnceLock};

pub struct RangeBuiltin {
    pub sheet_name: &'static str,
    pub rhai_name: &'static str,
    #[allow(dead_code)]
    pub description: &'static str,
}

pub const RANGE_BUILTINS: &[RangeBuiltin] = &[
    RangeBuiltin {
        sheet_name: "SUM",
        rhai_name: "sum_range",
        description: "Sum of numeric values in a cell range",
    },
    RangeBuiltin {
        sheet_name: "AVG",
        rhai_name: "avg_range",
        description: "Average of numeric values in a cell range",
    },
    RangeBuiltin {
        sheet_name: "COUNT",
        rhai_name: "count_range",
        description: "Count of non-empty cells in a cell range",
    },
    RangeBuiltin {
        sheet_name: "MIN",
        rhai_name: "min_range",
        description: "Minimum numeric value in a cell range",
    },
    RangeBuiltin {
        sheet_name: "MAX",
        rhai_name: "max_range",
        description: "Maximum numeric value in a cell range",
    },
    RangeBuiltin {
        sheet_name: "BARCHART",
        rhai_name: "barchart_range",
        description: "Render a bar chart for the given range",
    },
    RangeBuiltin {
        sheet_name: "LINECHART",
        rhai_name: "linechart_range",
        description: "Render a line chart for the given range",
    },
    RangeBuiltin {
        sheet_name: "SCATTER",
        rhai_name: "scatter_range",
        description: "Render a scatter plot for a 2-column range",
    },
    RangeBuiltin {
        sheet_name: "VEC",
        rhai_name: "vec_range",
        description: "Convert a range to an array",
    },
];

/// Regex that matches built-in range calls like `SUM(A1:B5)`.
///
/// Captures:
/// - group 1: function name (e.g. `SUM`)
/// - group 2: start cell ref (e.g. `A1`)
/// - group 3: end cell ref (e.g. `B5`)
pub fn range_fn_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let names = RANGE_BUILTINS
            .iter()
            .map(|b| b.sheet_name)
            .collect::<Vec<_>>()
            .join("|");
        Regex::new(&format!(
            r"\b({})\(([A-Za-z]+[0-9]+):([A-Za-z]+[0-9]+)(\s*,[^)]*)?\)",
            names
        ))
        .expect("built-in range regex must compile")
    })
}

pub fn range_rhai_name(sheet_name: &str) -> Option<&'static str> {
    RANGE_BUILTINS
        .iter()
        .find(|b| b.sheet_name == sheet_name)
        .map(|b| b.rhai_name)
}

fn eval_script_cell(ctx: &NativeCallContext, script: &str) -> Option<f64> {
    // `script` is stored without the leading '='.
    let processed = preprocess_script(script);
    let value = ctx.engine().eval::<Dynamic>(&processed).ok()?;

    if let Ok(n) = value.as_float() {
        return Some(n);
    }
    if let Ok(n) = value.as_int() {
        return Some(n as f64);
    }
    None
}

fn cell_value_or_zero(ctx: &NativeCallContext, grid: &Grid, row: usize, col: usize) -> f64 {
    let cell_ref = CellRef::new(row, col);
    let Some(cell) = grid.get(&cell_ref) else {
        return 0.0;
    };

    match &cell.contents {
        CellType::Number(n) => *n,
        CellType::Empty => 0.0,
        CellType::Script(s) => eval_script_cell(ctx, s).unwrap_or(0.0),
        _ => 0.0,
    }
}

#[allow(clippy::too_many_arguments)]
fn make_plot_spec(
    kind: PlotKind,
    r1: i64,
    c1: i64,
    r2: i64,
    c2: i64,
    title: Option<String>,
    x_label: Option<String>,
    y_label: Option<String>,
) -> String {
    let spec = PlotSpec {
        kind,
        r1: r1.min(r2) as usize,
        c1: c1.min(c2) as usize,
        r2: r1.max(r2) as usize,
        c2: c1.max(c2) as usize,
        title,
        x_label,
        y_label,
    };
    format_plot_spec(&spec)
}

/// Register all built-in functions into the Rhai engine.
pub fn register_builtins(engine: &mut Engine, grid: Arc<Grid>, spill_map: Arc<SpillMap>) {
    // cell(row, col): numeric value at cell (text/script -> NaN)
    // Checks spill map first for spilled array values
    let grid_cell = Arc::clone(&grid);
    let spill_cell = Arc::clone(&spill_map);
    engine.register_fn(
        "cell",
        move |ctx: NativeCallContext, row: i64, col: i64| -> f64 {
            let cell_ref = CellRef::new(row as usize, col as usize);

            // Check spill map first
            if let Some(spill_val) = spill_cell.get(&cell_ref) {
                if let Ok(n) = spill_val.as_float() {
                    return n;
                }
                if let Ok(n) = spill_val.as_int() {
                    return n as f64;
                }
                return f64::NAN;
            }

            if let Some(entry) = grid_cell.get(&cell_ref) {
                match &entry.contents {
                    CellType::Number(n) => *n,
                    CellType::Empty => 0.0,
                    CellType::Script(s) => eval_script_cell(&ctx, s).unwrap_or(f64::NAN),
                    _ => f64::NAN,
                }
            } else {
                0.0
            }
        },
    );

    // value(row, col): typed value at cell (number/text/bool) as Dynamic.
    // - Empty cells => "" (so things like `len(@A1)` behave intuitively)
    // - Script cells => evaluated result (or UNIT on evaluation error)
    // Checks spill map first for spilled array values
    let grid_value = Arc::clone(&grid);
    let spill_value = Arc::clone(&spill_map);
    engine.register_fn(
        "value",
        move |ctx: NativeCallContext, row: i64, col: i64| -> Dynamic {
            let cell_ref = CellRef::new(row as usize, col as usize);

            // Check spill map first
            if let Some(spill_val) = spill_value.get(&cell_ref) {
                return spill_val.clone();
            }
            let Some(entry) = grid_value.get(&cell_ref) else {
                return Dynamic::from("".to_string());
            };

            match &entry.contents {
                CellType::Empty => Dynamic::from("".to_string()),
                CellType::Number(n) => Dynamic::from(*n),
                CellType::Text(s) => Dynamic::from(s.clone()),
                CellType::Script(s) => {
                    let processed = preprocess_script(s);
                    ctx.engine()
                        .eval::<Dynamic>(&processed)
                        .unwrap_or(Dynamic::UNIT)
                }
            }
        },
    );

    // sum_range(r1, c1, r2, c2)
    let grid_sum = Arc::clone(&grid);
    engine.register_fn(
        "sum_range",
        move |ctx: NativeCallContext, r1: i64, c1: i64, r2: i64, c2: i64| -> f64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;
            let mut sum = 0.0;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    sum += cell_value_or_zero(&ctx, &grid_sum, row, col);
                }
            }
            sum
        },
    );

    // avg_range(r1, c1, r2, c2)
    let grid_avg = Arc::clone(&grid);
    engine.register_fn(
        "avg_range",
        move |ctx: NativeCallContext, r1: i64, c1: i64, r2: i64, c2: i64| -> f64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;
            let mut sum = 0.0;
            let mut count = 0;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    sum += cell_value_or_zero(&ctx, &grid_avg, row, col);
                    count += 1;
                }
            }
            if count > 0 { sum / count as f64 } else { 0.0 }
        },
    );

    // count_range(r1, c1, r2, c2): count non-empty
    let grid_count = Arc::clone(&grid);
    engine.register_fn(
        "count_range",
        move |_ctx: NativeCallContext, r1: i64, c1: i64, r2: i64, c2: i64| -> f64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;
            let mut count = 0;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    let cell_ref = CellRef::new(row, col);
                    if let Some(cell) = grid_count.get(&cell_ref)
                        && !matches!(cell.contents, CellType::Empty)
                    {
                        count += 1;
                    }
                }
            }
            count as f64
        },
    );

    // min_range(r1, c1, r2, c2)
    let grid_min = Arc::clone(&grid);
    engine.register_fn(
        "min_range",
        move |ctx: NativeCallContext, r1: i64, c1: i64, r2: i64, c2: i64| -> f64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;
            let mut min_val = f64::INFINITY;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    let val = cell_value_or_zero(&ctx, &grid_min, row, col);
                    if val < min_val {
                        min_val = val;
                    }
                }
            }
            if min_val == f64::INFINITY {
                0.0
            } else {
                min_val
            }
        },
    );

    // max_range(r1, c1, r2, c2)
    let grid_max = Arc::clone(&grid);
    engine.register_fn(
        "max_range",
        move |ctx: NativeCallContext, r1: i64, c1: i64, r2: i64, c2: i64| -> f64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;
            let mut max_val = f64::NEG_INFINITY;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    let val = cell_value_or_zero(&ctx, &grid_max, row, col);
                    if val > max_val {
                        max_val = val;
                    }
                }
            }
            if max_val == f64::NEG_INFINITY {
                0.0
            } else {
                max_val
            }
        },
    );

    // Plot specs (rendered by the TUI)
    engine.register_fn(
        "barchart_range",
        move |r1: i64, c1: i64, r2: i64, c2: i64| -> String {
            make_plot_spec(PlotKind::Bar, r1, c1, r2, c2, None, None, None)
        },
    );
    engine.register_fn(
        "barchart_range",
        move |r1: i64, c1: i64, r2: i64, c2: i64, title: String| -> String {
            make_plot_spec(PlotKind::Bar, r1, c1, r2, c2, Some(title), None, None)
        },
    );
    engine.register_fn(
        "barchart_range",
        move |r1: i64, c1: i64, r2: i64, c2: i64, title: String, x: String, y: String| -> String {
            make_plot_spec(PlotKind::Bar, r1, c1, r2, c2, Some(title), Some(x), Some(y))
        },
    );

    engine.register_fn(
        "linechart_range",
        move |r1: i64, c1: i64, r2: i64, c2: i64| -> String {
            make_plot_spec(PlotKind::Line, r1, c1, r2, c2, None, None, None)
        },
    );
    engine.register_fn(
        "linechart_range",
        move |r1: i64, c1: i64, r2: i64, c2: i64, title: String| -> String {
            make_plot_spec(PlotKind::Line, r1, c1, r2, c2, Some(title), None, None)
        },
    );
    engine.register_fn(
        "linechart_range",
        move |r1: i64, c1: i64, r2: i64, c2: i64, title: String, x: String, y: String| -> String {
            make_plot_spec(
                PlotKind::Line,
                r1,
                c1,
                r2,
                c2,
                Some(title),
                Some(x),
                Some(y),
            )
        },
    );

    engine.register_fn(
        "scatter_range",
        move |r1: i64, c1: i64, r2: i64, c2: i64| -> String {
            make_plot_spec(PlotKind::Scatter, r1, c1, r2, c2, None, None, None)
        },
    );
    engine.register_fn(
        "scatter_range",
        move |r1: i64, c1: i64, r2: i64, c2: i64, title: String| -> String {
            make_plot_spec(PlotKind::Scatter, r1, c1, r2, c2, Some(title), None, None)
        },
    );
    engine.register_fn(
        "scatter_range",
        move |r1: i64, c1: i64, r2: i64, c2: i64, title: String, x: String, y: String| -> String {
            make_plot_spec(
                PlotKind::Scatter,
                r1,
                c1,
                r2,
                c2,
                Some(title),
                Some(x),
                Some(y),
            )
        },
    );

    // sorted(array): returns a new sorted array (non-mutating alternative to .sort())
    engine.register_fn("sorted", |arr: rhai::Array| -> rhai::Array {
        let mut result = arr.clone();
        result.sort_by(|a, b| {
            // Compare as floats if both are numeric
            let a_num = a
                .as_float()
                .ok()
                .or_else(|| a.as_int().ok().map(|i| i as f64));
            let b_num = b
                .as_float()
                .ok()
                .or_else(|| b.as_int().ok().map(|i| i as f64));
            match (a_num, b_num) {
                (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                _ => {
                    // Fall back to string comparison
                    let a_str = a.to_string();
                    let b_str = b.to_string();
                    a_str.cmp(&b_str)
                }
            }
        });
        result
    });

    // OUTPUT(x): identity, but named to communicate "spill this" intent.
    // OUTPUT(x, f): calls f(x) and returns f's result (or x if f returns ())
    // This is useful for in-place operations like Rhai's Array.sort() which returns ().
    engine.register_fn("OUTPUT", |x: Dynamic| -> Dynamic { x });
    engine.register_fn(
        "OUTPUT",
        |ctx: NativeCallContext,
         x: Dynamic,
         f: FnPtr|
         -> Result<Dynamic, Box<rhai::EvalAltResult>> {
            let out = f.call_within_context::<Dynamic>(&ctx, (x.clone(),))?;
            if out.is_unit() { Ok(x) } else { Ok(out) }
        },
    );

    // reversed(array): returns a new reversed array (non-mutating alternative to .reverse())
    engine.register_fn("reversed", |arr: rhai::Array| -> rhai::Array {
        let mut result = arr.clone();
        result.reverse();
        result
    });

    // vec_range(r1, c1, r2, c2): returns array of cell values
    // Checks spill map first for spilled array values
    let grid_vec = Arc::clone(&grid);
    let spill_vec = Arc::clone(&spill_map);
    engine.register_fn(
        "vec_range",
        move |ctx: NativeCallContext, r1: i64, c1: i64, r2: i64, c2: i64| -> rhai::Array {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;

            let mut result = rhai::Array::new();
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    let cell_ref = CellRef::new(row, col);

                    // Check spill map first
                    let val = if let Some(spill_val) = spill_vec.get(&cell_ref) {
                        spill_val.clone()
                    } else if let Some(entry) = grid_vec.get(&cell_ref) {
                        match &entry.contents {
                            CellType::Number(n) => Dynamic::from(*n),
                            CellType::Text(s) => Dynamic::from(s.clone()),
                            CellType::Empty => Dynamic::from(0.0),
                            CellType::Script(s) => {
                                let processed = preprocess_script(s);
                                ctx.engine()
                                    .eval::<Dynamic>(&processed)
                                    .unwrap_or(Dynamic::UNIT)
                            }
                        }
                    } else {
                        Dynamic::from(0.0)
                    };
                    result.push(val);
                }
            }
            result
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Cell, Grid};
    use dashmap::DashMap;

    #[test]
    fn test_range_rhai_name_mapping() {
        assert_eq!(range_rhai_name("SUM"), Some("sum_range"));
        assert_eq!(range_rhai_name("AVG"), Some("avg_range"));
        assert_eq!(range_rhai_name("NOPE"), None);
    }

    #[test]
    fn test_range_regex_matches_uppercase_only() {
        let re = range_fn_re();
        assert!(re.is_match("SUM(A1:B2)"));
        assert!(!re.is_match("sum(A1:B2)"));
    }

    #[test]
    fn test_sum_range_uses_script_values() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(1.0));
        grid.insert(CellRef::new(1, 0), Cell::new_script("A1 + 1"));

        let mut engine = Engine::new();
        let spill_map: SpillMap = DashMap::new();
        register_builtins(&mut engine, Arc::new(grid), Arc::new(spill_map));

        let result: f64 = engine.eval("sum_range(0, 0, 1, 0)").unwrap();
        assert_eq!(result, 3.0);
    }

    #[test]
    fn test_plot_spec_builtins_return_tagged_string() {
        let grid: Grid = DashMap::new();
        let mut engine = Engine::new();
        let spill_map: SpillMap = DashMap::new();
        register_builtins(&mut engine, Arc::new(grid), Arc::new(spill_map));

        let s: String = engine.eval("barchart_range(0, 0, 9, 0)").unwrap();
        assert!(s.starts_with(crate::plot::PLOT_PREFIX));
    }

    #[test]
    fn test_vec_range_returns_array() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(1, 0), Cell::new_number(20.0));
        grid.insert(CellRef::new(2, 0), Cell::new_number(30.0));

        let mut engine = Engine::new();
        let spill_map: SpillMap = DashMap::new();
        register_builtins(&mut engine, Arc::new(grid), Arc::new(spill_map));

        // Test basic VEC returns array
        let result: rhai::Array = engine.eval("vec_range(0, 0, 2, 0)").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].clone().cast::<f64>(), 10.0);
        assert_eq!(result[1].clone().cast::<f64>(), 20.0);
        assert_eq!(result[2].clone().cast::<f64>(), 30.0);
    }

    #[test]
    fn test_vec_range_with_map() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(1, 0), Cell::new_number(20.0));

        let mut engine = Engine::new();
        let spill_map: SpillMap = DashMap::new();
        register_builtins(&mut engine, Arc::new(grid), Arc::new(spill_map));

        // Test VEC with map transformation
        let result: rhai::Array = engine.eval("vec_range(0, 0, 1, 0).map(|x| x * 2)").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].clone().cast::<f64>(), 20.0);
        assert_eq!(result[1].clone().cast::<f64>(), 40.0);
    }

    #[test]
    fn test_vec_range_with_filter() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(5.0));
        grid.insert(CellRef::new(1, 0), Cell::new_number(15.0));
        grid.insert(CellRef::new(2, 0), Cell::new_number(25.0));

        let mut engine = Engine::new();
        let spill_map: SpillMap = DashMap::new();
        register_builtins(&mut engine, Arc::new(grid), Arc::new(spill_map));

        // Test VEC with filter
        let result: rhai::Array = engine
            .eval("vec_range(0, 0, 2, 0).filter(|x| x > 10)")
            .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].clone().cast::<f64>(), 15.0);
        assert_eq!(result[1].clone().cast::<f64>(), 25.0);
    }

    #[test]
    fn test_vec_range_reads_from_spill_map() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));

        let spill_map: SpillMap = DashMap::new();
        // Simulate spill values at A2 and A3
        spill_map.insert(CellRef::new(1, 0), Dynamic::from(20.0_f64));
        spill_map.insert(CellRef::new(2, 0), Dynamic::from(30.0_f64));

        let mut engine = Engine::new();
        register_builtins(&mut engine, Arc::new(grid), Arc::new(spill_map));

        // VEC should read from both grid and spill_map
        let result: rhai::Array = engine.eval("vec_range(0, 0, 2, 0)").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].clone().cast::<f64>(), 10.0);
        assert_eq!(result[1].clone().cast::<f64>(), 20.0);
        assert_eq!(result[2].clone().cast::<f64>(), 30.0);
    }

    #[test]
    fn test_sorted_returns_new_array() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(30.0));
        grid.insert(CellRef::new(1, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(2, 0), Cell::new_number(20.0));

        let mut engine = Engine::new();
        let spill_map: SpillMap = DashMap::new();
        register_builtins(&mut engine, Arc::new(grid), Arc::new(spill_map));

        // sorted() should return a new sorted array
        let result: rhai::Array = engine.eval("sorted(vec_range(0, 0, 2, 0))").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].clone().cast::<f64>(), 10.0);
        assert_eq!(result[1].clone().cast::<f64>(), 20.0);
        assert_eq!(result[2].clone().cast::<f64>(), 30.0);
    }

    #[test]
    fn test_reversed_returns_new_array() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(1, 0), Cell::new_number(20.0));
        grid.insert(CellRef::new(2, 0), Cell::new_number(30.0));

        let mut engine = Engine::new();
        let spill_map: SpillMap = DashMap::new();
        register_builtins(&mut engine, Arc::new(grid), Arc::new(spill_map));

        // reversed() should return a new reversed array
        let result: rhai::Array = engine.eval("reversed(vec_range(0, 0, 2, 0))").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].clone().cast::<f64>(), 30.0);
        assert_eq!(result[1].clone().cast::<f64>(), 20.0);
        assert_eq!(result[2].clone().cast::<f64>(), 10.0);
    }

    #[test]
    fn test_output_returns_sorted_array_from_in_place_sort() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(30.0));
        grid.insert(CellRef::new(1, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(2, 0), Cell::new_number(20.0));

        let mut engine = Engine::new();
        let spill_map: SpillMap = DashMap::new();
        register_builtins(&mut engine, Arc::new(grid), Arc::new(spill_map));

        let result: rhai::Array = engine
            .eval("OUTPUT(vec_range(0, 0, 2, 0), |v| { v.sort(); v })")
            .unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].clone().cast::<f64>(), 10.0);
        assert_eq!(result[1].clone().cast::<f64>(), 20.0);
        assert_eq!(result[2].clone().cast::<f64>(), 30.0);
    }
}
