//! Built-in spreadsheet functions (Rust) and their metadata.
//!
//! Conventions:
//! - Spreadsheet-facing built-in names are ALL CAPS (e.g. `SUM`, `AVG`).
//! - Built-ins rewrite to lowercase Rhai function names (e.g. `sum_range`).
//! - If you add a new built-in range function, update `RANGE_BUILTINS` and
//!   register its implementation in `register_builtins`.

use crate::engine::{CellRef, CellType, Grid, ValueCache, preprocess_script};
use crate::plot::{PlotKind, PlotSpec, format_plot_spec};
use rand::Rng;
use regex::Regex;
use rhai::{Dynamic, Engine, FnPtr, NativeCallContext};
use std::sync::OnceLock;

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
    RangeBuiltin {
        sheet_name: "SUMIF",
        rhai_name: "sumif_range",
        description: "Sum values where predicate is true",
    },
    RangeBuiltin {
        sheet_name: "COUNTIF",
        rhai_name: "countif_range",
        description: "Count cells where predicate is true",
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
pub fn register_builtins(engine: &mut Engine, grid: Grid, value_cache: ValueCache) {
    // cell(row, col): numeric value at cell (text/script -> NaN)
    // Checks value cache first for pre-evaluated values
    let grid_cell = grid.clone();
    let cache_cell = value_cache.clone();
    engine.register_fn(
        "cell",
        move |ctx: NativeCallContext, row: i64, col: i64| -> f64 {
            let cell_ref = CellRef::new(row as usize, col as usize);

            // Check value cache first (for pre-computed formulas and spills)
            if let Some(cached_val) = cache_cell.get(&cell_ref) {
                if let Ok(n) = cached_val.as_float() {
                    return n;
                }
                if let Ok(n) = cached_val.as_int() {
                    return n as f64;
                }
                return f64::NAN;
            }

            if let Some(entry) = grid_cell.get(&cell_ref) {
                match &entry.contents {
                    CellType::Number(n) => *n,
                    CellType::Empty => 0.0,
                    CellType::Script(s) => {
                        // Fallback: try to evaluate (works for built-in-only scripts)
                        eval_script_cell(&ctx, s).unwrap_or(f64::NAN)
                    }
                    _ => f64::NAN,
                }
            } else {
                0.0
            }
        },
    );

    // value(row, col): typed value at cell (number/text/bool) as Dynamic.
    // - Empty cells => "" (so things like `len(@A1)` behave intuitively)
    // - Script cells => use cached value from value_cache, fall back to eval
    // Checks value cache first for pre-evaluated values
    let grid_value = grid.clone();
    let cache_value = value_cache.clone();
    engine.register_fn(
        "value",
        move |ctx: NativeCallContext, row: i64, col: i64| -> Dynamic {
            let cell_ref = CellRef::new(row as usize, col as usize);

            // Check value cache first (for pre-computed formulas and array spills)
            if let Some(cached_val) = cache_value.get(&cell_ref) {
                return cached_val.clone();
            }

            let Some(entry) = grid_value.get(&cell_ref) else {
                return Dynamic::from("".to_string());
            };

            match &entry.contents {
                CellType::Empty => Dynamic::from("".to_string()),
                CellType::Number(n) => Dynamic::from(*n),
                CellType::Text(s) => Dynamic::from(s.clone()),
                CellType::Script(s) => {
                    // Fallback: try to evaluate (works for built-in-only scripts)
                    let processed = preprocess_script(s);
                    ctx.engine()
                        .eval::<Dynamic>(&processed)
                        .unwrap_or(Dynamic::UNIT)
                }
            }
        },
    );

    // sum_range(r1, c1, r2, c2)
    let grid_sum = grid.clone();
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
    let grid_avg = grid.clone();
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
    let grid_count = grid.clone();
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
    let grid_min = grid.clone();
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
    let grid_max = grid.clone();
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

    // SPILL(x): converts ranges to arrays, identity for arrays.
    // Arrays automatically spill down when returned from a formula.
    // Function form: SPILL(0..=10) or SPILL(arr)
    engine.register_fn("SPILL", |arr: rhai::Array| -> rhai::Array { arr });
    engine.register_fn("SPILL", |range: std::ops::Range<i64>| -> rhai::Array {
        range.map(Dynamic::from).collect()
    });
    engine.register_fn(
        "SPILL",
        |range: std::ops::RangeInclusive<i64>| -> rhai::Array {
            range.map(Dynamic::from).collect()
        },
    );

    // Method form: (0..=10).SPILL() or arr.SPILL()
    engine.register_fn("SPILL", |arr: &mut rhai::Array| -> rhai::Array { arr.clone() });
    engine.register_fn(
        "SPILL",
        |range: &mut std::ops::Range<i64>| -> rhai::Array {
            range.clone().map(Dynamic::from).collect()
        },
    );
    engine.register_fn(
        "SPILL",
        |range: &mut std::ops::RangeInclusive<i64>| -> rhai::Array {
            range.clone().map(Dynamic::from).collect()
        },
    );

    // vec_range(r1, c1, r2, c2): returns array of cell values
    // Checks spill map first for spilled array values
    // Respects range direction: VEC(A3:A1) returns [A3, A2, A1]
    let grid_vec = grid.clone();
    let cache_vec = value_cache.clone();
    engine.register_fn(
        "vec_range",
        move |ctx: NativeCallContext, r1: i64, c1: i64, r2: i64, c2: i64| -> rhai::Array {
            // Build row/col indices respecting direction
            let rows: Vec<usize> = if r1 <= r2 {
                (r1 as usize..=r2 as usize).collect()
            } else {
                (r2 as usize..=r1 as usize).rev().collect()
            };
            let cols: Vec<usize> = if c1 <= c2 {
                (c1 as usize..=c2 as usize).collect()
            } else {
                (c2 as usize..=c1 as usize).rev().collect()
            };

            let mut result = rhai::Array::new();
            for row in &rows {
                for col in &cols {
                    let cell_ref = CellRef::new(*row, *col);

                    // Check value cache first
                    let val = if let Some(cached_val) = cache_vec.get(&cell_ref) {
                        cached_val.clone()
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

    // POW(base, exp): exponentiation
    // Rhai doesn't have built-in pow for floats, so we register it here
    // Handle all type combinations since cell values can be int or float
    engine.register_fn("POW", |base: f64, exp: f64| -> f64 { base.powf(exp) });
    engine.register_fn("POW", |base: f64, exp: i64| -> f64 { base.powf(exp as f64) });
    engine.register_fn("POW", |base: i64, exp: f64| -> f64 { (base as f64).powf(exp) });
    engine.register_fn("POW", |base: i64, exp: i64| -> f64 { (base as f64).powf(exp as f64) });

    // SQRT(x): square root
    engine.register_fn("SQRT", |x: f64| -> f64 { x.sqrt() });
    engine.register_fn("SQRT", |x: i64| -> f64 { (x as f64).sqrt() });

    // RAND(): random float in [0.0, 1.0)
    engine.register_fn("RAND", || -> f64 { rand::thread_rng().r#gen() });

    // RANDINT(min, max): random integer in [min, max] inclusive
    engine.register_fn("RANDINT", |min: i64, max: i64| -> i64 {
        rand::thread_rng().r#gen_range(min..=max)
    });

    // SUMIF(r1, c1, r2, c2, predicate): sum values where predicate returns true
    let grid_sumif = grid.clone();
    let cache_sumif = value_cache.clone();
    engine.register_fn(
        "sumif_range",
        move |ctx: NativeCallContext, r1: i64, c1: i64, r2: i64, c2: i64, pred: FnPtr| -> f64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;

            let mut sum = 0.0;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    let cell_ref = CellRef::new(row, col);

                    let val = if let Some(cached_val) = cache_sumif.get(&cell_ref) {
                        cached_val.clone()
                    } else if let Some(entry) = grid_sumif.get(&cell_ref) {
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

                    // Call the predicate
                    if let Ok(result) = pred.call_within_context::<bool>(&ctx, (val.clone(),)) {
                        if result {
                            if let Ok(n) = val.as_float() {
                                sum += n;
                            } else if let Ok(n) = val.as_int() {
                                sum += n as f64;
                            }
                        }
                    }
                }
            }
            sum
        },
    );

    // COUNTIF(r1, c1, r2, c2, predicate): count cells where predicate returns true
    let grid_countif = grid.clone();
    let cache_countif = value_cache.clone();
    engine.register_fn(
        "countif_range",
        move |ctx: NativeCallContext, r1: i64, c1: i64, r2: i64, c2: i64, pred: FnPtr| -> i64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;

            let mut count = 0i64;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    let cell_ref = CellRef::new(row, col);

                    let val = if let Some(cached_val) = cache_countif.get(&cell_ref) {
                        cached_val.clone()
                    } else if let Some(entry) = grid_countif.get(&cell_ref) {
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

                    // Call the predicate
                    if let Ok(result) = pred.call_within_context::<bool>(&ctx, (val,)) {
                        if result {
                            count += 1;
                        }
                    }
                }
            }
            count
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
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

        let result: f64 = engine.eval("sum_range(0, 0, 1, 0)").unwrap();
        assert_eq!(result, 3.0);
    }

    #[test]
    fn test_plot_spec_builtins_return_tagged_string() {
        let grid: Grid = DashMap::new();
        let mut engine = Engine::new();
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

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
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

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
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

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
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

        // Test VEC with filter
        let result: rhai::Array = engine
            .eval("vec_range(0, 0, 2, 0).filter(|x| x > 10)")
            .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].clone().cast::<f64>(), 15.0);
        assert_eq!(result[1].clone().cast::<f64>(), 25.0);
    }

    #[test]
    fn test_vec_range_reads_from_value_cache() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));

        let value_cache = ValueCache::new();
        // Simulate cached values at A2 and A3
        value_cache.insert(CellRef::new(1, 0), Dynamic::from(20.0_f64));
        value_cache.insert(CellRef::new(2, 0), Dynamic::from(30.0_f64));

        let mut engine = Engine::new();
        register_builtins(&mut engine, grid, value_cache);

        // VEC should read from both grid and value_cache
        let result: rhai::Array = engine.eval("vec_range(0, 0, 2, 0)").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].clone().cast::<f64>(), 10.0);
        assert_eq!(result[1].clone().cast::<f64>(), 20.0);
        assert_eq!(result[2].clone().cast::<f64>(), 30.0);
    }

    #[test]
    fn test_spill_array_identity() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(1, 0), Cell::new_number(20.0));
        grid.insert(CellRef::new(2, 0), Cell::new_number(30.0));

        let mut engine = Engine::new();
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

        // SPILL on array returns the same array
        let result: rhai::Array = engine.eval("SPILL(vec_range(0, 0, 2, 0))").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].clone().cast::<f64>(), 10.0);
        assert_eq!(result[1].clone().cast::<f64>(), 20.0);
        assert_eq!(result[2].clone().cast::<f64>(), 30.0);
    }

    #[test]
    fn test_spill_exclusive_range() {
        let grid: Grid = DashMap::new();
        let mut engine = Engine::new();
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

        // SPILL on exclusive range (0..5) converts to array [0,1,2,3,4]
        let result: rhai::Array = engine.eval("SPILL(0..5)").unwrap();
        assert_eq!(result.len(), 5);
        assert_eq!(result[0].clone().cast::<i64>(), 0);
        assert_eq!(result[4].clone().cast::<i64>(), 4);
    }

    #[test]
    fn test_spill_inclusive_range() {
        let grid: Grid = DashMap::new();
        let mut engine = Engine::new();
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

        // SPILL on inclusive range (0..=5) converts to array [0,1,2,3,4,5]
        let result: rhai::Array = engine.eval("SPILL(0..=5)").unwrap();
        assert_eq!(result.len(), 6);
        assert_eq!(result[0].clone().cast::<i64>(), 0);
        assert_eq!(result[5].clone().cast::<i64>(), 5);
    }

    #[test]
    fn test_spill_method_form() {
        let grid: Grid = DashMap::new();
        let mut engine = Engine::new();
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

        // Method form: (0..=3).SPILL()
        let result: rhai::Array = engine.eval("(0..=3).SPILL()").unwrap();
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].clone().cast::<i64>(), 0);
        assert_eq!(result[3].clone().cast::<i64>(), 3);
    }

    #[test]
    fn test_vec_range_respects_reverse_direction() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(1, 0), Cell::new_number(20.0));
        grid.insert(CellRef::new(2, 0), Cell::new_number(30.0));

        let mut engine = Engine::new();
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

        // Forward direction: A1:A3 = [10, 20, 30]
        let forward: rhai::Array = engine.eval("vec_range(0, 0, 2, 0)").unwrap();
        assert_eq!(forward.len(), 3);
        assert_eq!(forward[0].clone().cast::<f64>(), 10.0);
        assert_eq!(forward[1].clone().cast::<f64>(), 20.0);
        assert_eq!(forward[2].clone().cast::<f64>(), 30.0);

        // Reverse direction: A3:A1 = [30, 20, 10]
        let reverse: rhai::Array = engine.eval("vec_range(2, 0, 0, 0)").unwrap();
        assert_eq!(reverse.len(), 3);
        assert_eq!(reverse[0].clone().cast::<f64>(), 30.0);
        assert_eq!(reverse[1].clone().cast::<f64>(), 20.0);
        assert_eq!(reverse[2].clone().cast::<f64>(), 10.0);
    }

    #[test]
    fn test_rand_returns_value_in_range() {
        let grid: Grid = DashMap::new();
        let mut engine = Engine::new();
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

        // RAND() should return a value in [0.0, 1.0)
        for _ in 0..100 {
            let result: f64 = engine.eval("RAND()").unwrap();
            assert!(result >= 0.0 && result < 1.0);
        }
    }

    #[test]
    fn test_randint_returns_value_in_range() {
        let grid: Grid = DashMap::new();
        let mut engine = Engine::new();
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

        // RANDINT(1, 6) should return a value in [1, 6]
        for _ in 0..100 {
            let result: i64 = engine.eval("RANDINT(1, 6)").unwrap();
            assert!(result >= 1 && result <= 6);
        }
    }

    #[test]
    fn test_sumif_range() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(1, 0), Cell::new_number(20.0));
        grid.insert(CellRef::new(2, 0), Cell::new_number(30.0));
        grid.insert(CellRef::new(3, 0), Cell::new_number(5.0));

        let mut engine = Engine::new();
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

        // Sum values > 10: 20 + 30 = 50
        let result: f64 = engine.eval("sumif_range(0, 0, 3, 0, |x| x > 10)").unwrap();
        assert_eq!(result, 50.0);

        // Sum values <= 10: 10 + 5 = 15
        let result: f64 = engine.eval("sumif_range(0, 0, 3, 0, |x| x <= 10)").unwrap();
        assert_eq!(result, 15.0);
    }

    #[test]
    fn test_countif_range() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(1, 0), Cell::new_number(20.0));
        grid.insert(CellRef::new(2, 0), Cell::new_number(30.0));
        grid.insert(CellRef::new(3, 0), Cell::new_number(5.0));

        let mut engine = Engine::new();
        let value_cache = ValueCache::new();
        register_builtins(&mut engine, grid, value_cache);

        // Count values > 10: 20, 30 = 2
        let result: i64 = engine.eval("countif_range(0, 0, 3, 0, |x| x > 10)").unwrap();
        assert_eq!(result, 2);

        // Count values >= 10: 10, 20, 30 = 3
        let result: i64 = engine.eval("countif_range(0, 0, 3, 0, |x| x >= 10)").unwrap();
        assert_eq!(result, 3);
    }
}
