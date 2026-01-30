//! Spreadsheet engine API.

mod cell;
mod cell_ref;
mod cycle;
mod deps;
mod eval;
mod format;
mod preprocess;

pub use cell::{Cell, CellType, Grid, SpillMap};
pub use cell_ref::CellRef;
pub use cycle::detect_cycle;
pub use deps::{extract_dependencies, parse_range};
pub use eval::{
    create_engine, create_engine_with_functions, create_engine_with_functions_and_spill,
    create_engine_with_spill, eval_with_functions,
};
pub use format::{format_dynamic, format_number};
pub use preprocess::{ShiftOperation, preprocess_script, shift_formula_references};

pub use rhai::{AST, Dynamic};
