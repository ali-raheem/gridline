//! Built-in spreadsheet functions (Rust) and their metadata.
//!
//! Conventions:
//! - Spreadsheet-facing built-in names are ALL CAPS (e.g. `SUM`, `AVG`).
//! - Built-ins rewrite to ALLCAPS Rhai function names (e.g. `SUM_RANGE`).
//! - If you add a new built-in range function, update `RANGE_BUILTINS` and
//!   register its implementation in `register_builtins`.

use crate::engine::{Cell, CellRef, CellType, Grid, ValueCache, parse_range, preprocess_script};
use crate::plot::{PlotKind, PlotSpec, format_plot_spec};
use rand::Rng;
use regex::Regex;
use rhai::{Dynamic, Engine, EvalAltResult, FnPtr, NativeCallContext, Position};

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

pub struct RangeBuiltin {
    pub sheet_name: &'static str,
    pub rhai_name: &'static str,
    #[allow(dead_code)]
    pub description: &'static str,
}

pub const RANGE_BUILTINS: &[RangeBuiltin] = &[
    RangeBuiltin {
        sheet_name: "SUM",
        rhai_name: "SUM_RANGE",
        description: "Sum of numeric values in a cell range",
    },
    RangeBuiltin {
        sheet_name: "AVG",
        rhai_name: "AVG_RANGE",
        description: "Average of numeric values in a cell range",
    },
    RangeBuiltin {
        sheet_name: "COUNT",
        rhai_name: "COUNT_RANGE",
        description: "Count of non-empty cells in a cell range",
    },
    RangeBuiltin {
        sheet_name: "MIN",
        rhai_name: "MIN_RANGE",
        description: "Minimum numeric value in a cell range",
    },
    RangeBuiltin {
        sheet_name: "MAX",
        rhai_name: "MAX_RANGE",
        description: "Maximum numeric value in a cell range",
    },
    RangeBuiltin {
        sheet_name: "BARCHART",
        rhai_name: "BARCHART_RANGE",
        description: "Render a bar chart for the given range",
    },
    RangeBuiltin {
        sheet_name: "LINECHART",
        rhai_name: "LINECHART_RANGE",
        description: "Render a line chart for the given range",
    },
    RangeBuiltin {
        sheet_name: "SCATTER",
        rhai_name: "SCATTER_RANGE",
        description: "Render a scatter plot for a 2-column range",
    },
    RangeBuiltin {
        sheet_name: "VEC",
        rhai_name: "VEC_RANGE",
        description: "Convert a range to an array",
    },
    RangeBuiltin {
        sheet_name: "SUMIF",
        rhai_name: "SUMIF_RANGE",
        description: "Sum values where predicate is true",
    },
    RangeBuiltin {
        sheet_name: "COUNTIF",
        rhai_name: "COUNTIF_RANGE",
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

fn invalid_arg(message: &str) -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime(message.into(), Position::NONE).into()
}

fn to_usize(value: i64, label: &str) -> Result<usize, Box<EvalAltResult>> {
    usize::try_from(value).map_err(|_| invalid_arg(&format!("{} must be >= 0", label)))
}

fn to_decimal_places(value: i64) -> Result<usize, Box<EvalAltResult>> {
    const MAX_DECIMALS: usize = 12;
    let places = to_usize(value, "decimals")?;
    if places > MAX_DECIMALS {
        return Err(invalid_arg(&format!(
            "decimals must be <= {}",
            MAX_DECIMALS
        )));
    }
    Ok(places)
}

fn fixed_decimal_string(n: f64, decimals: usize) -> String {
    if n.is_nan() {
        return "#NAN!".to_string();
    }
    if n.is_infinite() {
        return "#INF!".to_string();
    }

    // Fixed number of decimal places (always prints trailing zeros).
    format!("{:.*}", decimals, n)
}

fn money_string(n: f64, symbol: &str, decimals: usize) -> String {
    if n.is_nan() {
        return "#NAN!".to_string();
    }
    if n.is_infinite() {
        return "#INF!".to_string();
    }

    let sign = if n.is_sign_negative() { "-" } else { "" };
    let abs = n.abs();
    format!("{}{}{}", sign, symbol, fixed_decimal_string(abs, decimals))
}

fn cell_value_or_zero(
    ctx: &NativeCallContext,
    grid: &Grid,
    value_cache: &ValueCache,
    col: usize,
    row: usize,
) -> f64 {
    let cell_ref = CellRef::new(col, row);

    // Check value cache first (for pre-computed formulas and spills)
    if let Some(cached_val) = value_cache.get(&cell_ref) {
        if let Ok(n) = cached_val.as_float() {
            return n;
        }
        if let Ok(n) = cached_val.as_int() {
            return n as f64;
        }
        return 0.0;
    }

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
    // CELL(col, row): numeric value at cell (text/script -> NaN)

    // Checks value cache first for pre-evaluated values
    let grid_cell = grid.clone();
    let cache_cell = value_cache.clone();
    engine.register_fn(
        "CELL",
        move |ctx: NativeCallContext, col: i64, row: i64| -> f64 {
            let cell_ref = CellRef::new(col as usize, row as usize);

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

    // VALUE(col, row): typed value at cell (number/text/bool) as Dynamic.

    // - Empty cells => "" (so things like `len(@A1)` behave intuitively)
    // - Script cells => use cached value from value_cache, fall back to eval
    // Checks value cache first for pre-evaluated values
    let grid_value = grid.clone();
    let cache_value = value_cache.clone();
    engine.register_fn(
        "VALUE",
        move |ctx: NativeCallContext, col: i64, row: i64| -> Dynamic {
            let cell_ref = CellRef::new(col as usize, row as usize);

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

    // SUM_RANGE(c1, r1, c2, r2)

    let grid_sum = grid.clone();
    let cache_sum = value_cache.clone();
    engine.register_fn(
        "SUM_RANGE",
        move |ctx: NativeCallContext, c1: i64, r1: i64, c2: i64, r2: i64| -> f64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;
            let mut sum = 0.0;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    sum += cell_value_or_zero(&ctx, &grid_sum, &cache_sum, col, row);
                }
            }
            sum
        },
    );

    // AVG_RANGE(c1, r1, c2, r2)

    let grid_avg = grid.clone();
    let cache_avg = value_cache.clone();
    engine.register_fn(
        "AVG_RANGE",
        move |ctx: NativeCallContext, c1: i64, r1: i64, c2: i64, r2: i64| -> f64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;
            let mut sum = 0.0;
            let mut count = 0;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    sum += cell_value_or_zero(&ctx, &grid_avg, &cache_avg, col, row);
                    count += 1;
                }
            }
            if count > 0 { sum / count as f64 } else { 0.0 }
        },
    );

    // COUNT_RANGE(c1, r1, c2, r2): count non-empty

    let grid_count = grid.clone();
    let cache_count = value_cache.clone();
    engine.register_fn(
        "COUNT_RANGE",
        move |_ctx: NativeCallContext, c1: i64, r1: i64, c2: i64, r2: i64| -> f64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;
            let mut count = 0;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    let cell_ref = CellRef::new(col, row);
                    if cache_count.contains_key(&cell_ref) {
                        count += 1;
                        continue;
                    }
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

    // MIN_RANGE(c1, r1, c2, r2)

    let grid_min = grid.clone();
    let cache_min = value_cache.clone();
    engine.register_fn(
        "MIN_RANGE",
        move |ctx: NativeCallContext, c1: i64, r1: i64, c2: i64, r2: i64| -> f64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;
            let mut min_val = f64::INFINITY;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    let val = cell_value_or_zero(&ctx, &grid_min, &cache_min, col, row);
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

    // MAX_RANGE(c1, r1, c2, r2)

    let grid_max = grid.clone();
    let cache_max = value_cache.clone();
    engine.register_fn(
        "MAX_RANGE",
        move |ctx: NativeCallContext, c1: i64, r1: i64, c2: i64, r2: i64| -> f64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;
            let mut max_val = f64::NEG_INFINITY;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    let val = cell_value_or_zero(&ctx, &grid_max, &cache_max, col, row);
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
        "BARCHART_RANGE",
        move |c1: i64, r1: i64, c2: i64, r2: i64| -> String {
            make_plot_spec(PlotKind::Bar, r1, c1, r2, c2, None, None, None)
        },
    );
    engine.register_fn(
        "BARCHART_RANGE",
        move |c1: i64, r1: i64, c2: i64, r2: i64, title: String| -> String {
            make_plot_spec(PlotKind::Bar, r1, c1, r2, c2, Some(title), None, None)
        },
    );
    engine.register_fn(
        "BARCHART_RANGE",
        move |c1: i64, r1: i64, c2: i64, r2: i64, title: String, x: String, y: String| -> String {
            make_plot_spec(PlotKind::Bar, r1, c1, r2, c2, Some(title), Some(x), Some(y))
        },
    );

    engine.register_fn(
        "LINECHART_RANGE",
        move |c1: i64, r1: i64, c2: i64, r2: i64| -> String {
            make_plot_spec(PlotKind::Line, r1, c1, r2, c2, None, None, None)
        },
    );
    engine.register_fn(
        "LINECHART_RANGE",
        move |c1: i64, r1: i64, c2: i64, r2: i64, title: String| -> String {
            make_plot_spec(PlotKind::Line, r1, c1, r2, c2, Some(title), None, None)
        },
    );
    engine.register_fn(
        "LINECHART_RANGE",
        move |c1: i64, r1: i64, c2: i64, r2: i64, title: String, x: String, y: String| -> String {
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
        "SCATTER_RANGE",
        move |c1: i64, r1: i64, c2: i64, r2: i64| -> String {
            make_plot_spec(PlotKind::Scatter, r1, c1, r2, c2, None, None, None)
        },
    );
    engine.register_fn(
        "SCATTER_RANGE",
        move |c1: i64, r1: i64, c2: i64, r2: i64, title: String| -> String {
            make_plot_spec(PlotKind::Scatter, r1, c1, r2, c2, Some(title), None, None)
        },
    );
    engine.register_fn(
        "SCATTER_RANGE",
        move |c1: i64, r1: i64, c2: i64, r2: i64, title: String, x: String, y: String| -> String {
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

    // PARSE_CELL("A1"): returns [col, row] (0-indexed)
    engine.register_fn(
        "PARSE_CELL",
        |cell_str: &str| -> Result<rhai::Array, Box<EvalAltResult>> {
            let Some(cell_ref) = CellRef::from_str(cell_str) else {
                return Err(invalid_arg(&format!(
                    "Invalid cell reference: {}",
                    cell_str
                )));
            };
            Ok(vec![
                Dynamic::from(cell_ref.col as i64),
                Dynamic::from(cell_ref.row as i64),
            ])
        },
    );

    // FORMAT_CELL(col, row): returns "A1" (0-indexed)

    engine.register_fn(
        "FORMAT_CELL",
        |col: i64, row: i64| -> Result<String, Box<EvalAltResult>> {
            let col = to_usize(col, "col")?;
            let row = to_usize(row, "row")?;
            Ok(CellRef::new(col, row).to_string())
        },
    );

    // PARSE_RANGE("A1:B4"): returns [c1, r1, c2, r2] (0-indexed, col/row)

    engine.register_fn(
        "PARSE_RANGE",
        |range: &str| -> Result<rhai::Array, Box<EvalAltResult>> {
            let Some((c1, r1, c2, r2)) = parse_range(range) else {
                return Err(invalid_arg(&format!("Invalid range reference: {}", range)));
            };
            Ok(vec![
                Dynamic::from(c1 as i64),
                Dynamic::from(r1 as i64),
                Dynamic::from(c2 as i64),
                Dynamic::from(r2 as i64),
            ])
        },
    );

    // FORMAT_RANGE(c1, r1, c2, r2): returns "A1:B4" (0-indexed)

    engine.register_fn(
        "FORMAT_RANGE",
        |c1: i64, r1: i64, c2: i64, r2: i64| -> Result<String, Box<EvalAltResult>> {
            let c1 = to_usize(c1, "c1")?;
            let r1 = to_usize(r1, "r1")?;
            let c2 = to_usize(c2, "c2")?;
            let r2 = to_usize(r2, "r2")?;
            Ok(format!("{}:{}", CellRef::new(c1, r1), CellRef::new(c2, r2)))
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
    engine.register_fn("SPILL", |arr: &mut rhai::Array| -> rhai::Array {
        arr.clone()
    });
    engine.register_fn("SPILL", |range: &mut std::ops::Range<i64>| -> rhai::Array {
        range.clone().map(Dynamic::from).collect()
    });
    engine.register_fn(
        "SPILL",
        |range: &mut std::ops::RangeInclusive<i64>| -> rhai::Array {
            range.clone().map(Dynamic::from).collect()
        },
    );

    // VEC_RANGE(c1, r1, c2, r2): returns array of cell values

    // Checks spill map first for spilled array values
    // Respects range direction: VEC(A3:A1) returns [A3, A2, A1]
    let grid_vec = grid.clone();
    let cache_vec = value_cache.clone();
    engine.register_fn(
        "VEC_RANGE",
        move |ctx: NativeCallContext, c1: i64, r1: i64, c2: i64, r2: i64| -> rhai::Array {
            // Build col/row indices respecting direction

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
                    let cell_ref = CellRef::new(*col, *row);

                    // Check value cache first
                    let val = if let Some(cached_val) = cache_vec.get(&cell_ref) {
                        cached_val.clone()
                    } else if let Some(entry) = grid_vec.get(&cell_ref) {
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
                    } else {
                        Dynamic::from("".to_string())
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
    engine.register_fn("POW", |base: f64, exp: i64| -> f64 {
        base.powf(exp as f64)
    });
    engine.register_fn("POW", |base: i64, exp: f64| -> f64 {
        (base as f64).powf(exp)
    });
    engine.register_fn("POW", |base: i64, exp: i64| -> f64 {
        (base as f64).powf(exp as f64)
    });

    // SQRT(x): square root
    engine.register_fn("SQRT", |x: f64| -> f64 { x.sqrt() });
    engine.register_fn("SQRT", |x: i64| -> f64 { (x as f64).sqrt() });

    // RAND(): random float in [0.0, 1.0)
    engine.register_fn("RAND", || -> f64 { rand::thread_rng().r#gen() });

    // RANDINT(min, max): random integer in [min, max] inclusive
    engine.register_fn("RANDINT", |min: i64, max: i64| -> i64 {
        rand::thread_rng().r#gen_range(min..=max)
    });

    // FIXED(n, decimals): format with a fixed number of decimal places.
    engine.register_fn(
        "FIXED",
        |n: f64, decimals: i64| -> Result<String, Box<EvalAltResult>> {
            let decimals = to_decimal_places(decimals)?;
            Ok(fixed_decimal_string(n, decimals))
        },
    );
    engine.register_fn(
        "FIXED",
        |n: i64, decimals: i64| -> Result<String, Box<EvalAltResult>> {
            let decimals = to_decimal_places(decimals)?;
            Ok(fixed_decimal_string(n as f64, decimals))
        },
    );

    // MONEY(n, symbol[, decimals]): format as currency (no separators).
    // Examples:
    //   MONEY(15.0424, "£")    -> "£15.04"
    //   MONEY(-2, "$", 0)      -> "-$2"
    engine.register_fn("MONEY", |n: f64, symbol: &str| -> String {
        money_string(n, symbol, 2)
    });
    engine.register_fn("MONEY", |n: i64, symbol: &str| -> String {
        money_string(n as f64, symbol, 2)
    });
    engine.register_fn(
        "MONEY",
        |n: f64, symbol: &str, decimals: i64| -> Result<String, Box<EvalAltResult>> {
            let decimals = to_decimal_places(decimals)?;
            Ok(money_string(n, symbol, decimals))
        },
    );
    engine.register_fn(
        "MONEY",
        |n: i64, symbol: &str, decimals: i64| -> Result<String, Box<EvalAltResult>> {
            let decimals = to_decimal_places(decimals)?;
            Ok(money_string(n as f64, symbol, decimals))
        },
    );

    // SUMIF(c1, r1, c2, r2, predicate): sum values where predicate returns true
    let grid_sumif = grid.clone();
    let cache_sumif = value_cache.clone();
    engine.register_fn(
        "SUMIF_RANGE",
        move |ctx: NativeCallContext, c1: i64, r1: i64, c2: i64, r2: i64, pred: FnPtr| -> f64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;
            let mut sum = 0.0;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    let _cell_ref = CellRef::new(col, row);
                    let val = cell_value_or_zero(&ctx, &grid_sumif, &cache_sumif, col, row);
                    let pred_result: bool = pred.call_within_context(&ctx, (val,)).unwrap_or(false);
                    if pred_result {
                        sum += val;
                    }
                }
            }
            sum
        },
    );

    // COUNTIF(c1, r1, c2, r2, predicate): count cells where predicate returns true
    let grid_countif = grid.clone();
    let cache_countif = value_cache.clone();
    engine.register_fn(
        "COUNTIF_RANGE",
        move |ctx: NativeCallContext, c1: i64, r1: i64, c2: i64, r2: i64, pred: FnPtr| -> i64 {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;
            let mut count = 0;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    let _cell_ref = CellRef::new(col, row);
                    let val = cell_value_or_zero(&ctx, &grid_countif, &cache_countif, col, row);
                    let pred_result: bool = pred.call_within_context(&ctx, (val,)).unwrap_or(false);
                    if pred_result {
                        count += 1;
                    }
                }
            }
            count
        },
    );
}

/// Tracks cell modifications made by script builtins.
/// Maps CellRef -> (old_cell, new_cell) to support undo.
pub type ScriptModifications = Arc<Mutex<HashMap<CellRef, (Option<Cell>, Option<Cell>)>>>;

/// Register script-only write builtins for script execution.
/// These are NOT available in cell formulas, only when running scripts via :call or :rhai.
pub fn register_script_builtins(
    engine: &mut Engine,
    grid: Grid,
    modifications: ScriptModifications,
) {
    // SET_CELL(col, row, value) - Set cell by column/row indices
    let grid_set = grid.clone();
    let mods_set = modifications.clone();
    engine.register_fn("SET_CELL", move |col: i64, row: i64, value: Dynamic| {
        let cell_ref = CellRef::new(col as usize, row as usize);
        let new_cell = dynamic_to_cell(value);

        let old_cell = grid_set.get(&cell_ref).map(|r| r.clone());
        grid_set.insert(cell_ref.clone(), new_cell.clone());

        let mut mods = mods_set.lock().unwrap();
        mods.entry(cell_ref)
            .and_modify(|(_, nc)| *nc = Some(new_cell.clone()))
            .or_insert((old_cell, Some(new_cell)));
    });

    // SET_CELL("A1", value) - Set cell by A1 notation
    let grid_set_a1 = grid.clone();
    let mods_set_a1 = modifications.clone();
    engine.register_fn("SET_CELL", move |cell_str: &str, value: Dynamic| {
        let Some(cell_ref) = CellRef::from_str(cell_str) else {
            return; // Invalid cell reference - silently ignore
        };
        let old_cell = grid_set_a1.get(&cell_ref).map(|r| r.clone());

        let new_cell = dynamic_to_cell(value);
        grid_set_a1.insert(cell_ref.clone(), new_cell.clone());

        let mut mods = mods_set_a1.lock().unwrap();
        mods.entry(cell_ref)
            .and_modify(|(_, nc)| *nc = Some(new_cell.clone()))
            .or_insert((old_cell, Some(new_cell)));
    });

    // CLEAR_CELL(col, row) - Clear cell by column/row indices
    let grid_clear = grid.clone();
    let mods_clear = modifications.clone();
    engine.register_fn("CLEAR_CELL", move |col: i64, row: i64| {
        let cell_ref = CellRef::new(col as usize, row as usize);
        let old_cell = grid_clear.get(&cell_ref).map(|r| r.clone());
        grid_clear.remove(&cell_ref);

        let mut mods = mods_clear.lock().unwrap();
        mods.entry(cell_ref)
            .and_modify(|(_, nc)| *nc = None)
            .or_insert((old_cell, None));
    });

    // CLEAR_CELL("A1") - Clear cell by A1 notation
    let grid_clear_a1 = grid.clone();
    let mods_clear_a1 = modifications.clone();
    engine.register_fn("CLEAR_CELL", move |cell_str: &str| {
        let Some(cell_ref) = CellRef::from_str(cell_str) else {
            return;
        };
        let old_cell = grid_clear_a1.get(&cell_ref).map(|r| r.clone());
        grid_clear_a1.remove(&cell_ref);

        let mut mods = mods_clear_a1.lock().unwrap();
        mods.entry(cell_ref)
            .and_modify(|(_, nc)| *nc = None)
            .or_insert((old_cell, None));
    });

    // SET_RANGE(c1, r1, c2, r2, value) - Fill range with value
    let grid_set_range = grid.clone();
    let mods_set_range = modifications.clone();
    engine.register_fn(
        "SET_RANGE",
        move |c1: i64, r1: i64, c2: i64, r2: i64, value: Dynamic| {
            let min_row = r1.min(r2) as usize;
            let max_row = r1.max(r2) as usize;
            let min_col = c1.min(c2) as usize;
            let max_col = c1.max(c2) as usize;

            let new_cell = dynamic_to_cell(value);

            let mut mods = mods_set_range.lock().unwrap();
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    let cell_ref = CellRef::new(col, row);
                    let old_cell = grid_set_range.get(&cell_ref).map(|r| r.clone());
                    grid_set_range.insert(cell_ref.clone(), new_cell.clone());

                    mods.entry(cell_ref)
                        .and_modify(|(_, nc)| *nc = Some(new_cell.clone()))
                        .or_insert((old_cell, Some(new_cell.clone())));
                }
            }
        },
    );

    // CLEAR_RANGE(c1, r1, c2, r2) - Clear a range of cells
    let grid_clear_range = grid.clone();
    let mods_clear_range = modifications.clone();
    engine.register_fn("CLEAR_RANGE", move |c1: i64, r1: i64, c2: i64, r2: i64| {
        let min_row = r1.min(r2) as usize;
        let max_row = r1.max(r2) as usize;
        let min_col = c1.min(c2) as usize;
        let max_col = c1.max(c2) as usize;

        let mut mods = mods_clear_range.lock().unwrap();
        for row in min_row..=max_row {
            for col in min_col..=max_col {
                let cell_ref = CellRef::new(col, row);
                let old_cell = grid_clear_range.get(&cell_ref).map(|r| r.clone());
                grid_clear_range.remove(&cell_ref);

                mods.entry(cell_ref)
                    .and_modify(|(_, nc)| *nc = None)
                    .or_insert((old_cell, None));
            }
        }
    });
}

/// Convert a Rhai Dynamic value to a Cell.
fn dynamic_to_cell(value: Dynamic) -> Cell {
    if value.is_unit() {
        return Cell::new_empty();
    }
    if let Ok(s) = value.clone().into_string() {
        // Check if it's a formula
        if s.starts_with('=') {
            return Cell::new_script(&s[1..]);
        }
        return Cell::new_text(&s);
    }
    if let Ok(n) = value.as_float() {
        return Cell::new_number(n);
    }
    if let Ok(n) = value.as_int() {
        return Cell::new_number(n as f64);
    }
    if let Ok(b) = value.as_bool() {
        return Cell::new_text(if b { "TRUE" } else { "FALSE" });
    }
    // Fallback: convert to string
    Cell::new_text(&value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Cell, Grid};
    use dashmap::DashMap;

    #[test]
    fn test_range_rhai_name_mapping() {
        assert_eq!(range_rhai_name("SUM"), Some("SUM_RANGE"));
        assert_eq!(range_rhai_name("AVG"), Some("AVG_RANGE"));
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
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_number(1.0));
        grid.insert(CellRef::new(0, 1), Cell::new_script("A1 + 1"));

        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        let result: f64 = engine.eval("SUM_RANGE(0, 0, 0, 1)").unwrap();
        assert_eq!(result, 3.0);
    }

    #[test]
    fn test_sum_range_prefers_value_cache() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_script("unknown_func()"));

        let value_cache = ValueCache::default();
        value_cache.insert(CellRef::new(0, 0), Dynamic::from(5.0_f64));

        let mut engine = Engine::new();
        register_builtins(&mut engine, grid, value_cache);

        let result: f64 = engine.eval("SUM_RANGE(0, 0, 0, 0)").unwrap();
        assert_eq!(result, 5.0);
    }

    #[test]
    fn test_plot_spec_builtins_return_tagged_string() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        let s: String = engine.eval("BARCHART_RANGE(0, 0, 0, 9)").unwrap();
        assert!(s.starts_with(crate::plot::PLOT_PREFIX));
    }

    #[test]
    fn test_vec_range_returns_array() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(0, 1), Cell::new_number(20.0));
        grid.insert(CellRef::new(0, 2), Cell::new_number(30.0));

        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        // Test basic VEC returns array
        let result: rhai::Array = engine.eval("VEC_RANGE(0, 0, 0, 2)").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].clone().cast::<f64>(), 10.0);
        assert_eq!(result[1].clone().cast::<f64>(), 20.0);
        assert_eq!(result[2].clone().cast::<f64>(), 30.0);
    }

    #[test]
    fn test_parse_cell_and_format_cell() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        let result: rhai::Array = engine.eval("PARSE_CELL(\"A1\")").unwrap();
        assert_eq!(result[0].clone().cast::<i64>(), 0);
        assert_eq!(result[1].clone().cast::<i64>(), 0);

        let result: rhai::Array = engine.eval("PARSE_CELL(\"B4\")").unwrap();
        assert_eq!(result[0].clone().cast::<i64>(), 1);
        assert_eq!(result[1].clone().cast::<i64>(), 3);

        let result: String = engine.eval("FORMAT_CELL(1, 3)").unwrap();
        assert_eq!(result, "B4");
    }

    #[test]
    fn test_parse_range_and_format_range() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        let result: rhai::Array = engine.eval("PARSE_RANGE(\"A1:B4\")").unwrap();
        assert_eq!(result[0].clone().cast::<i64>(), 0);
        assert_eq!(result[1].clone().cast::<i64>(), 0);
        assert_eq!(result[2].clone().cast::<i64>(), 1);
        assert_eq!(result[3].clone().cast::<i64>(), 3);

        let result: String = engine.eval("FORMAT_RANGE(0, 0, 1, 3)").unwrap();
        assert_eq!(result, "A1:B4");
    }

    #[test]
    fn test_parse_cell_invalid_reference() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        let result: Result<rhai::Array, _> = engine.eval("PARSE_CELL(\"A0\")");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_range_invalid_reference() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        let result: Result<rhai::Array, _> = engine.eval("PARSE_RANGE(\"A1\")");
        assert!(result.is_err());
    }

    #[test]
    fn test_format_cell_rejects_negative_indices() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        let result: Result<String, _> = engine.eval("FORMAT_CELL(-1, 0)");
        assert!(result.is_err());
    }

    #[test]
    fn test_format_range_rejects_negative_indices() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        let result: Result<String, _> = engine.eval("FORMAT_RANGE(-1, 0, 2, 2)");
        assert!(result.is_err());
    }

    #[test]
    fn test_vec_range_with_map() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(0, 1), Cell::new_number(20.0));

        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        // Test VEC with map transformation
        let result: rhai::Array = engine.eval("VEC_RANGE(0, 0, 0, 1).map(|x| x * 2)").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].clone().cast::<f64>(), 20.0);
        assert_eq!(result[1].clone().cast::<f64>(), 40.0);
    }

    #[test]
    fn test_vec_range_with_filter() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_number(5.0));
        grid.insert(CellRef::new(0, 1), Cell::new_number(15.0));
        grid.insert(CellRef::new(0, 2), Cell::new_number(25.0));

        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        // Test VEC with filter
        let result: rhai::Array = engine
            .eval("VEC_RANGE(0, 0, 0, 2).filter(|x| x > 10)")
            .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].clone().cast::<f64>(), 15.0);
        assert_eq!(result[1].clone().cast::<f64>(), 25.0);
    }

    #[test]
    fn test_vec_range_reads_from_value_cache() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));

        let value_cache = ValueCache::default();
        value_cache.insert(CellRef::new(0, 1), Dynamic::from(20.0_f64));
        value_cache.insert(CellRef::new(0, 2), Dynamic::from(30.0_f64));

        let mut engine = Engine::new();
        register_builtins(&mut engine, grid, value_cache);

        let result: rhai::Array = engine.eval("VEC_RANGE(0, 0, 0, 2)").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].clone().cast::<f64>(), 10.0);
        assert_eq!(result[1].clone().cast::<f64>(), 20.0);
        assert_eq!(result[2].clone().cast::<f64>(), 30.0);
    }

    #[test]
    fn test_spill_array_identity() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(0, 1), Cell::new_number(20.0));
        grid.insert(CellRef::new(0, 2), Cell::new_number(30.0));

        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        // SPILL on array returns the same array
        let result: rhai::Array = engine.eval("SPILL(VEC_RANGE(0, 0, 0, 2))").unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].clone().cast::<f64>(), 10.0);
        assert_eq!(result[1].clone().cast::<f64>(), 20.0);
        assert_eq!(result[2].clone().cast::<f64>(), 30.0);
    }

    #[test]
    fn test_spill_exclusive_range() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        // SPILL on exclusive range (0..5) converts to array [0,1,2,3,4]
        let result: rhai::Array = engine.eval("SPILL(0..5)").unwrap();
        assert_eq!(result.len(), 5);
        assert_eq!(result[0].clone().cast::<i64>(), 0);
        assert_eq!(result[4].clone().cast::<i64>(), 4);
    }

    #[test]
    fn test_spill_inclusive_range() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        // SPILL on inclusive range (0..=5) converts to array [0,1,2,3,4,5]
        let result: rhai::Array = engine.eval("SPILL(0..=5)").unwrap();
        assert_eq!(result.len(), 6);
        assert_eq!(result[0].clone().cast::<i64>(), 0);
        assert_eq!(result[5].clone().cast::<i64>(), 5);
    }

    #[test]
    fn test_spill_method_form() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        // Method form: (0..=3).SPILL()
        let result: rhai::Array = engine.eval("(0..=3).SPILL()").unwrap();
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].clone().cast::<i64>(), 0);
        assert_eq!(result[3].clone().cast::<i64>(), 3);
    }

    #[test]
    fn test_vec_range_respects_reverse_direction() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(0, 1), Cell::new_number(20.0));
        grid.insert(CellRef::new(0, 2), Cell::new_number(30.0));

        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        // Forward direction: A1:A3 = [10, 20, 30]
        let forward: rhai::Array = engine.eval("VEC_RANGE(0, 0, 0, 2)").unwrap();
        assert_eq!(forward.len(), 3);
        assert_eq!(forward[0].clone().cast::<f64>(), 10.0);
        assert_eq!(forward[2].clone().cast::<f64>(), 30.0);

        // Reverse direction: A3:A1 = [30, 20, 10]
        let reverse: rhai::Array = engine.eval("VEC_RANGE(0, 2, 0, 0)").unwrap();
        assert_eq!(reverse.len(), 3);
        assert_eq!(reverse[0].clone().cast::<f64>(), 30.0);
        assert_eq!(reverse[2].clone().cast::<f64>(), 10.0);
    }

    #[test]
    fn test_rand_returns_value_in_range() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        // RAND() should return a value in [0.0, 1.0)
        for _ in 0..100 {
            let result: f64 = engine.eval("RAND()").unwrap();
            assert!(result >= 0.0 && result < 1.0);
        }
    }

    #[test]
    fn test_randint_returns_value_in_range() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        // RANDINT(1, 6) should return a value in [1, 6]
        for _ in 0..100 {
            let result: i64 = engine.eval("RANDINT(1, 6)").unwrap();
            assert!(result >= 1 && result <= 6);
        }
    }

    #[test]
    fn test_sumif_range() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(0, 1), Cell::new_number(20.0));
        grid.insert(CellRef::new(0, 2), Cell::new_number(30.0));
        grid.insert(CellRef::new(0, 3), Cell::new_number(5.0));

        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        // Sum values > 10: 20 + 30 = 50
        let result: f64 = engine.eval("SUMIF_RANGE(0, 0, 0, 3, |x| x > 10)").unwrap();
        assert_eq!(result, 50.0);

        // Sum values <= 10: 10 + 5 = 15
        let result: f64 = engine.eval("SUMIF_RANGE(0, 0, 0, 3, |x| x <= 10)").unwrap();
        assert_eq!(result, 15.0);
    }

    #[test]
    fn test_countif_range() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(0, 1), Cell::new_number(20.0));
        grid.insert(CellRef::new(0, 2), Cell::new_number(30.0));
        grid.insert(CellRef::new(0, 3), Cell::new_number(5.0));

        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        // Count values > 10: 20, 30 = 2
        let result: i64 = engine
            .eval("COUNTIF_RANGE(0, 0, 0, 3, |x| x > 10)")
            .unwrap();
        assert_eq!(result, 2);

        // Count values >= 10: 10, 20, 30 = 3
        let result: i64 = engine
            .eval("COUNTIF_RANGE(0, 0, 0, 3, |x| x >= 10)")
            .unwrap();
        assert_eq!(result, 3);
    }

    #[test]
    fn test_countif_range_col_row_order() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_number(3.0));
        grid.insert(CellRef::new(1, 0), Cell::new_number(4.0));
        grid.insert(CellRef::new(0, 1), Cell::new_number(20.0));
        grid.insert(CellRef::new(1, 1), Cell::new_number(30.0));

        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        let result: i64 = engine
            .eval("COUNTIF_RANGE(0, 1, 1, 1, |x| x >= 20)")
            .unwrap();
        assert_eq!(result, 2);
    }

    #[test]
    fn test_sumif_range_col_row_order() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_number(3.0));
        grid.insert(CellRef::new(1, 0), Cell::new_number(4.0));
        grid.insert(CellRef::new(0, 1), Cell::new_number(20.0));
        grid.insert(CellRef::new(1, 1), Cell::new_number(30.0));

        let mut engine = Engine::new();
        let value_cache = ValueCache::default();
        register_builtins(&mut engine, grid, value_cache);

        let result: f64 = engine.eval("SUMIF_RANGE(0, 1, 1, 1, |x| x >= 20)").unwrap();
        assert_eq!(result, 50.0);
    }

    #[test]
    fn test_script_builtins_set_cell() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let value_cache = ValueCache::default();
        let modifications: ScriptModifications =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

        let mut engine = Engine::new();
        register_builtins(&mut engine, grid.clone(), value_cache);
        register_script_builtins(&mut engine, grid.clone(), modifications.clone());

        // Set a cell using col/row
        let _: () = engine.eval("SET_CELL(0, 0, 42)").unwrap();
        let cell = grid.get(&CellRef::new(0, 0)).unwrap();
        assert!(matches!(cell.contents, CellType::Number(n) if (n - 42.0).abs() < 0.001));

        // Set a cell using A1 notation
        let _: () = engine.eval(r#"SET_CELL("B2", "hello")"#).unwrap();
        let cell = grid.get(&CellRef::new(1, 1)).unwrap();
        assert!(matches!(&cell.contents, CellType::Text(s) if s == "hello"));

        // Check modifications were tracked
        let mods = modifications.lock().unwrap();
        assert_eq!(mods.len(), 2);
    }

    #[test]
    fn test_script_builtins_clear_cell() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(1, 1), Cell::new_text("hello"));

        let value_cache = ValueCache::default();
        let modifications: ScriptModifications =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

        let mut engine = Engine::new();
        register_builtins(&mut engine, grid.clone(), value_cache);
        register_script_builtins(&mut engine, grid.clone(), modifications.clone());

        // Clear by col/row
        let _: () = engine.eval("CLEAR_CELL(0, 0)").unwrap();
        assert!(grid.get(&CellRef::new(0, 0)).is_none());

        // Clear by A1 notation
        let _: () = engine.eval(r#"CLEAR_CELL("B2")"#).unwrap();
        assert!(grid.get(&CellRef::new(1, 1)).is_none());
    }

    #[test]
    fn test_script_builtins_set_range() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let value_cache = ValueCache::default();
        let modifications: ScriptModifications =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

        let mut engine = Engine::new();
        register_builtins(&mut engine, grid.clone(), value_cache);
        register_script_builtins(&mut engine, grid.clone(), modifications.clone());

        // Fill a 3x2 range with value 99
        let _: () = engine.eval("SET_RANGE(0, 0, 1, 2, 99)").unwrap();

        // Verify all 6 cells were set
        for row in 0..=2 {
            for col in 0..=1 {
                let cell = grid.get(&CellRef::new(col, row)).unwrap();
                assert!(matches!(cell.contents, CellType::Number(n) if (n - 99.0).abs() < 0.001));
            }
        }
    }

    #[test]
    fn test_script_builtins_clear_range() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        // Fill a 2x2 grid
        for row in 0..=1 {
            for col in 0..=1 {
                grid.insert(
                    CellRef::new(col, row),
                    Cell::new_number(row as f64 + col as f64),
                );
            }
        }

        let value_cache = ValueCache::default();
        let modifications: ScriptModifications =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

        let mut engine = Engine::new();
        register_builtins(&mut engine, grid.clone(), value_cache);
        register_script_builtins(&mut engine, grid.clone(), modifications.clone());

        // Clear the range
        let _: () = engine.eval("CLEAR_RANGE(0, 0, 1, 1)").unwrap();

        // Verify all cells were cleared
        for row in 0..=1 {
            for col in 0..=1 {
                assert!(grid.get(&CellRef::new(col, row)).is_none());
            }
        }
    }

    #[test]
    fn test_script_builtins_set_cell_formula() {
        let grid: Grid = std::sync::Arc::new(DashMap::new());
        let value_cache = ValueCache::default();
        let modifications: ScriptModifications =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

        let mut engine = Engine::new();
        register_builtins(&mut engine, grid.clone(), value_cache);
        register_script_builtins(&mut engine, grid.clone(), modifications);

        // Set a formula cell
        let _: () = engine.eval(r#"SET_CELL(0, 0, "=A2+B2")"#).unwrap();
        let cell = grid.get(&CellRef::new(0, 0)).unwrap();
        assert!(matches!(&cell.contents, CellType::Script(s) if s == "A2+B2"));
    }
}
