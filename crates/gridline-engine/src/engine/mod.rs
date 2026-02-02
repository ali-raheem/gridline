//! Spreadsheet engine API.
//!
//! This module provides the core computation engine for the spreadsheet:
//!
//! - [`Cell`], [`CellType`], [`Grid`] - Data structures for cell storage
//! - [`CellRef`] - Cell reference parsing (A1 notation ↔ row/col indices)
//! - [`detect_cycle`] - Circular dependency detection
//! - [`extract_dependencies`] - Parse formula dependencies
//! - [`preprocess_script`] - Transform formulas for Rhai evaluation
//! - [`create_engine`] - Create a Rhai engine with built-in functions
//! - [`format_dynamic`] - Format values for display

mod cell;
mod cell_ref;
mod cycle;
mod deps;
mod eval;
mod format;
mod preprocess;

pub use cell::{Cell, CellType, Grid, ValueCache};
// Legacy exports for backward compatibility
#[allow(unused_imports)]
pub use cell::{ComputedMap, SpillMap};
pub use cell_ref::CellRef;
pub use cycle::detect_cycle;
pub use deps::{extract_dependencies, parse_range};
pub use eval::{
    create_engine, create_engine_with_cache, create_engine_with_functions,
    create_engine_with_functions_and_cache, create_script_engine,
    create_script_engine_with_functions, eval_with_functions, eval_with_functions_script,
};
// Legacy exports for backward compatibility
#[allow(unused_imports)]
pub use eval::{create_engine_with_functions_and_spill, create_engine_with_spill};
pub use format::{format_dynamic, format_number};
pub use preprocess::{ShiftOperation, preprocess_script, preprocess_script_with_context, shift_formula_references};

pub use rhai::{AST, Dynamic};
